//! Parsing del contenedor MPQ para replays de StarCraft II.
//!
//! Un archivo `.SC2Replay` es un archivo MPQ envuelto en un `MPQUserData`
//! header. Este módulo se encarga únicamente de la capa de contenedor:
//! localizar el header MPQ real y sus tablas asociadas (hash table, block
//! table). No interpreta todavía el contenido de los archivos internos
//! (`replay.details`, `replay.tracker.events`, etc.) — eso vive en otro
//! módulo más adelante.

/// Errores que pueden ocurrir al parsear el contenedor MPQ de un replay.
///
/// Se distingue entre "el archivo es demasiado corto para contener el
/// campo que buscamos" y "la signature no es la esperada", porque son
/// fallos con causas y remedios distintos para quien use esta librería.
#[derive(Debug, thiserror::Error)]
pub enum MpqParseError {
    #[error(
        "se necesitaban {needed} bytes en offset {offset}, pero solo hay {available} disponibles"
    )]
    UnexpectedEof {
        needed: usize,
        offset: usize,
        available: usize,
    },
    #[error("signature inválida: se esperaba {expected:02x?}, se encontró {found:02x?}")]
    InvalidSignature { expected: [u8; 4], found: [u8; 4] },
}

/// Resultado corto para este módulo.
pub type Result<T> = std::result::Result<T, MpqParseError>;

// --- Constantes de layout ---------------------------------------------
//
// Nombradas explícitamente en vez de usar números mágicos en los rangos
// de slice. Los offsets son relativos al inicio de cada estructura, no
// al inicio del archivo.

const USER_DATA_SIGNATURE: [u8; 4] = *b"MPQ\x1b";
const MPQ_HEADER_SIGNATURE: [u8; 4] = *b"MPQ\x1a";

mod user_data_offsets {
    pub const SIGNATURE: (usize, usize) = (0, 4);
    pub const USER_DATA_SIZE: (usize, usize) = (4, 8);
    pub const HEADER_OFFSET: (usize, usize) = (8, 12);
    pub const USER_DATA_HEADER_SIZE: (usize, usize) = (12, 16);
}

mod header_offsets {
    pub const SIGNATURE: (usize, usize) = (0, 4);
    pub const HEADER_SIZE: (usize, usize) = (4, 8);
    pub const ARCHIVE_SIZE: (usize, usize) = (8, 12);
    pub const FORMAT_VERSION: (usize, usize) = (12, 14);
    pub const BLOCK_SIZE: (usize, usize) = (14, 16);
    pub const HASH_TABLE_POSITION: (usize, usize) = (16, 20);
    pub const BLOCK_TABLE_POSITION: (usize, usize) = (20, 24);
    pub const HASH_TABLE_SIZE: (usize, usize) = (24, 28);
    pub const BLOCK_TABLE_SIZE: (usize, usize) = (28, 32);
}

// --- Helpers de lectura --------------------------------------------------

/// Lee un `u32` little-endian del rango `(start, end)` dentro de `bytes`.
///
/// Centraliza el patrón `slice -> try_into -> from_le_bytes` que se repetía
/// en cada campo, y convierte un posible fallo de tamaño en un
/// `MpqParseError` en vez de un panic.
fn read_u32(bytes: &[u8], range: (usize, usize)) -> Result<u32> {
    let (start, end) = range;
    let slice = bytes.get(start..end).ok_or(MpqParseError::UnexpectedEof {
        needed: end - start,
        offset: start,
        available: bytes.len().saturating_sub(start),
    })?;
    // El slice viene garantizado en longitud 4 por el rango de la constante,
    // así que este unwrap es seguro: si falla, es un bug interno nuestro,
    // no una condición del archivo de entrada.
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

/// Lee un `u16` little-endian del rango `(start, end)` dentro de `bytes`.
fn read_u16(bytes: &[u8], range: (usize, usize)) -> Result<u16> {
    let (start, end) = range;
    let slice = bytes.get(start..end).ok_or(MpqParseError::UnexpectedEof {
        needed: end - start,
        offset: start,
        available: bytes.len().saturating_sub(start),
    })?;
    Ok(u16::from_le_bytes(slice.try_into().unwrap()))
}

fn read_signature(bytes: &[u8], range: (usize, usize)) -> Result<[u8; 4]> {
    let (start, end) = range;
    let slice = bytes.get(start..end).ok_or(MpqParseError::UnexpectedEof {
        needed: end - start,
        offset: start,
        available: bytes.len().saturating_sub(start),
    })?;
    Ok(slice.try_into().unwrap())
}

// --- Tipos de dominio -----------------------------------------------------

/// Envoltorio `MPQUserData` que precede al header MPQ real en un replay
/// de StarCraft II.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpqUserDataHeader {
    pub user_data_size: u32,
    /// Offset, relativo al inicio del archivo, donde empieza el header
    /// MPQ real (`MPQ\x1A`).
    pub header_offset: u32,
    pub user_data_header_size: u32,
}

impl MpqUserDataHeader {
    /// Parsea el `MPQUserData` a partir del inicio de un archivo de replay.
    ///
    /// # Errores
    /// Devuelve [`MpqParseError::InvalidSignature`] si los primeros 4 bytes
    /// no son `MPQ\x1B`, y [`MpqParseError::UnexpectedEof`] si `bytes` es
    /// demasiado corto.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let signature = read_signature(bytes, user_data_offsets::SIGNATURE)?;
        if signature != USER_DATA_SIGNATURE {
            return Err(MpqParseError::InvalidSignature {
                expected: USER_DATA_SIGNATURE,
                found: signature,
            });
        }

        Ok(Self {
            user_data_size: read_u32(bytes, user_data_offsets::USER_DATA_SIZE)?,
            header_offset: read_u32(bytes, user_data_offsets::HEADER_OFFSET)?,
            user_data_header_size: read_u32(bytes, user_data_offsets::USER_DATA_HEADER_SIZE)?,
        })
    }
}

/// Header MPQ real (formato V1-V4). Solo se exponen los campos necesarios
/// para localizar la hash table y la block table; el resto de la cabecera
/// extendida de V4 (hi-block table, checksums) queda fuera de alcance por
/// ahora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpqHeader {
    pub archive_size: u32,
    pub format_version: u16,
    /// Offset de la hash table, relativo al inicio del header MPQ
    /// (no al inicio del archivo).
    pub hash_table_position: u32,
    /// Offset de la block table, relativo al inicio del header MPQ.
    pub block_table_position: u32,
    /// Número de entradas de la hash table (no bytes).
    pub hash_table_size: u32,
    /// Número de entradas de la block table (no bytes).
    pub block_table_size: u32,
}

impl MpqHeader {
    /// Parsea el header MPQ a partir del offset indicado por
    /// [`MpqUserDataHeader::header_offset`].
    ///
    /// `bytes` debe ser el slice del archivo completo *a partir de* ese
    /// offset (es decir, ya recortado por quien llama).
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let signature = read_signature(bytes, header_offsets::SIGNATURE)?;
        if signature != MPQ_HEADER_SIGNATURE {
            return Err(MpqParseError::InvalidSignature {
                expected: MPQ_HEADER_SIGNATURE,
                found: signature,
            });
        }

        Ok(Self {
            archive_size: read_u32(bytes, header_offsets::ARCHIVE_SIZE)?,
            format_version: read_u16(bytes, header_offsets::FORMAT_VERSION)?,
            hash_table_position: read_u32(bytes, header_offsets::HASH_TABLE_POSITION)?,
            block_table_position: read_u32(bytes, header_offsets::BLOCK_TABLE_POSITION)?,
            hash_table_size: read_u32(bytes, header_offsets::HASH_TABLE_SIZE)?,
            block_table_size: read_u32(bytes, header_offsets::BLOCK_TABLE_SIZE)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_user_data() -> Vec<u8> {
        // MPQ\x1B, user_data_size=512, header_offset=1024, header_size=114
        let mut bytes = vec![0x4d, 0x50, 0x51, 0x1b];
        bytes.extend_from_slice(&512u32.to_le_bytes());
        bytes.extend_from_slice(&1024u32.to_le_bytes());
        bytes.extend_from_slice(&114u32.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_valid_user_data_header() {
        let bytes = sample_user_data();
        let header = MpqUserDataHeader::parse(&bytes).unwrap();

        assert_eq!(header.user_data_size, 512);
        assert_eq!(header.header_offset, 1024);
        assert_eq!(header.user_data_header_size, 114);
    }

    #[test]
    fn rejects_invalid_signature() {
        let mut bytes = sample_user_data();
        bytes[3] = 0x00; // corrompe la signature

        let err = MpqUserDataHeader::parse(&bytes).unwrap_err();
        assert!(matches!(err, MpqParseError::InvalidSignature { .. }));
    }

    #[test]
    fn reports_eof_instead_of_panicking() {
        let bytes = [0x4d, 0x50, 0x51, 0x1b]; // solo la signature, sin el resto
        let err = MpqUserDataHeader::parse(&bytes).unwrap_err();
        assert!(matches!(err, MpqParseError::UnexpectedEof { .. }));
    }
}
