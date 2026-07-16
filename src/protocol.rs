//! Decoding primitives for SC2's "versioned" protocol encoding
//! (`VersionedDecoder` in Blizzard's reference implementation).
//!
//! Every value in this format is prefixed by a one-byte tag identifying
//! its type at runtime, which is what allows a decoder to skip unknown
//! or newer fields ([`skip_value`]) without knowing the exact struct
//! layout of every protocol version in advance.

/// Errors that can occur while decoding the SC2 "versioned" protocol
/// format (used by `replay.details` and `replay.tracker.events`).
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// A value's type tag didn't match any of the known encodings
    /// (array, blob, optional, struct, or fixed-width int/vint).
    #[error("unsupported tag {tag:#04x} at position {pos}")]
    UnsupportedTag { tag: u8, pos: usize },
}

/// Reads a variable-length signed integer (`vint`) from `bytes` starting
/// at `*pos`, advancing `*pos` past the bytes consumed.
///
/// Encoding: the first byte holds the sign in bit 0 and 6 data bits in
/// bits 1-6. If bit 7 is set, another byte follows, contributing 7 more
/// data bits each time, until a byte with bit 7 unset is read.
pub fn read_vint(bytes: &[u8], pos: &mut usize) -> i64 {
    let mut b0 = next_byte(bytes, pos) as u64;
    let is_negative = (b0 & 1) == 1;
    let mut res: u64 = (b0 >> 1) & 0x3F;
    let mut bits_read = 6;

    while (b0 & 0x80) != 0 {
        b0 = next_byte(bytes, pos) as u64;
        res |= (b0 & 0x7F) << bits_read;
        bits_read += 7;
    }

    if is_negative {
        -(res as i64)
    } else {
        res as i64
    }
}

/// Reads a single byte from `bytes` at `*pos`, advancing `*pos` by one.
fn next_byte(bytes: &[u8], pos: &mut usize) -> u8 {
    let byte = bytes[*pos];
    *pos += 1;
    byte
}

/// Reads a length-prefixed blob (`0x02` tag + vint length + raw bytes).
pub fn read_blob<'a>(bytes: &'a [u8], pos: &mut usize) -> &'a [u8] {
    let _tag = next_byte(bytes, pos);
    let long = read_vint(bytes, pos) as usize;
    let start = *pos;
    let res = &bytes[start..start + long];
    *pos += long;
    res
}

/// Reads an optional value (`0x04` tag + presence byte + inner value).
///
/// Calls `read_inner` at most once, only if the presence byte is nonzero.
pub fn read_optional<T>(
    bytes: &[u8],
    pos: &mut usize,
    read_inner: impl FnOnce(&[u8], &mut usize) -> T,
) -> Option<T> {
    let _tag = next_byte(bytes, pos);
    let present = next_byte(bytes, pos);

    if present != 0 {
        Some(read_inner(bytes, pos))
    } else {
        None
    }
}

/// Reads a length-prefixed array (`0x00` tag + vint length + N elements),
/// calling `read_inner` once per element.
pub fn read_array<T>(
    bytes: &[u8],
    pos: &mut usize,
    mut read_inner: impl FnMut(&[u8], &mut usize) -> T,
) -> Vec<T> {
    let _tag = next_byte(bytes, pos);
    let long = read_vint(bytes, pos);
    let mut res = Vec::new();

    for _ in 0..long {
        res.push(read_inner(bytes, pos));
    }

    res
}

/// Reads a struct (`0x05` tag + vint field count + N (field_index, value)
/// pairs), calling `on_field` once per field with its index so the caller
/// can decide how to decode each one.
pub fn read_struct(
    bytes: &[u8],
    pos: &mut usize,
    mut on_field: impl FnMut(&[u8], &mut usize, i64),
) {
    let _tag = next_byte(bytes, pos);
    let num_fields = read_vint(bytes, pos);

    for _ in 0..num_fields {
        let field_index = read_vint(bytes, pos);
        on_field(bytes, pos, field_index);
    }
}

/// Skips over a single tagged value of any recognized type, advancing
/// `*pos` past it without needing to know its contents.
///
/// Recurses into `array` and `struct` (skipping each element/field in
/// turn) and into `optional` (only if present). Used by callers that only
/// care about a handful of fields in a struct and need to correctly skip
/// past the rest — see [`crate::details::decode_player`] for an example.
///
/// # Errors
/// Returns [`ProtocolError::UnsupportedTag`] if a tag outside the known
/// set (`0x00`-`0x09`) is encountered — currently `bitarray` (`0x01`) and
/// `choice` (`0x03`) aren't handled.
pub fn skip_value(bytes: &[u8], pos: &mut usize) -> Result<(), ProtocolError> {
    let tag = next_byte(bytes, pos);

    match tag {
        0x00 => {
            let long = read_vint(bytes, pos);
            for _ in 0..long {
                skip_value(bytes, pos)?;
            }
        }
        0x02 => {
            let long = read_vint(bytes, pos);
            *pos += long as usize;
        }
        0x04 => {
            let present = next_byte(bytes, pos);
            if present != 0 {
                skip_value(bytes, pos)?;
            }
        }
        0x05 => {
            let fields_number = read_vint(bytes, pos);
            for _ in 0..fields_number {
                read_vint(bytes, pos);
                skip_value(bytes, pos)?;
            }
        }
        0x06 => *pos += 1,
        0x07 => *pos += 4,
        0x08 => *pos += 8,
        0x09 => {
            read_vint(bytes, pos);
        }
        other => {
            return Err(ProtocolError::UnsupportedTag {
                tag: other,
                pos: *pos - 1,
            });
        }
    }

    Ok(())
}

pub fn read_choice_as_int(bytes: &[u8], pos: &mut usize) -> i64 {
    let _tag = next_byte(bytes, pos);
    let _selector = read_vint(bytes, pos);

    read_tagged_int(bytes, pos)
}

pub fn read_tagged_int(bytes: &[u8], pos: &mut usize) -> i64 {
    let tag = next_byte(bytes, pos);

    match tag {
        0x06 => next_byte(bytes, pos) as i64,
        0x07 => {
            let bytes4: [u8; 4] = bytes[*pos..*pos + 4].try_into().unwrap();
            *pos += 4;
            u32::from_le_bytes(bytes4) as i64
        }
        0x08 => {
            let bytes8: [u8; 8] = bytes[*pos..*pos + 8].try_into().unwrap();
            *pos += 8;
            u64::from_le_bytes(bytes8) as i64
        }
        0x09 => read_vint(bytes, pos),
        _ => unimplemented!("cannot read tagged int"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_zero() {
        let bytes = [0x00];
        let mut pos = 0;
        assert_eq!(read_vint(&bytes, &mut pos), 0);
        assert_eq!(pos, 1);
    }

    #[test]
    fn reads_blob() {
        // tag(blob), vint(2), 'A', 'B'
        let bytes = [0x02, 0x04, 0x41, 0x42];
        let mut pos = 0;

        let result = read_blob(&bytes, &mut pos);

        assert_eq!(result, [0x41, 0x42]);
        assert_eq!(pos, 4);
    }

    #[test]
    fn reads_array() {
        // tag(array), vint(2), 'A', 'B'
        let bytes = [0x00, 0x04, 0x41, 0x42];
        let mut pos = 0;

        let result = read_array(&bytes, &mut pos, |b, p| next_byte(b, p));

        assert_eq!(result, vec![0x41, 0x42]);
        assert_eq!(pos, 4);
    }

    #[test]
    fn reads_optional_present() {
        // tag(optional), present=1, value
        let bytes = [0x04, 0x01, 0x2A];
        let mut pos = 0;

        let value = read_optional(&bytes, &mut pos, |b, p| next_byte(b, p));

        assert_eq!(value, Some(0x2A));
        assert_eq!(pos, 3);
    }

    #[test]
    fn reads_optional_absent() {
        // tag(optional), present=0
        let bytes = [0x04, 0x00];
        let mut pos = 0;

        let value: Option<u8> = read_optional(&bytes, &mut pos, |b, p| next_byte(b, p));

        assert_eq!(value, None);
        assert_eq!(pos, 2);
    }

    #[test]
    fn reads_struct_with_two_fields() {
        let bytes = [0x05, 0x04, 0x00, 0x41, 0x02, 0x42];
        let mut pos = 0;

        let mut field_0 = 0u8;
        let mut field_1 = 0u8;

        read_struct(&bytes, &mut pos, |b, p, field_index| match field_index {
            0 => field_0 = next_byte(b, p),
            1 => field_1 = next_byte(b, p),
            _ => {}
        });

        assert_eq!(field_0, 0x41);
        assert_eq!(field_1, 0x42);
        assert_eq!(pos, 6);
    }

    #[test]
    fn skips_struct_with_nested_blob() {
        let bytes = [
            0x05, 0x02, // tag(struct), num_fields=1
            0x00, // field_index=0
            0x02, 0x04, 0x41, 0x42, // tag(blob), vint(2), 'A', 'B'
            0xFF,
        ];
        let mut pos = 0;

        skip_value(&bytes, &mut pos);

        assert_eq!(pos, bytes.len() - 1);
    }

    #[test]
    fn reads_choice_as_int() {
        // tag(choice)=0x03, selector=vint(0), tag(vint)=0x09, valor=vint(5)
        let bytes = [0x03, 0x00, 0x09, 0x0A]; // 5<<1 = 0x0A
        let mut pos = 0;

        let result = read_choice_as_int(&bytes, &mut pos);

        assert_eq!(result, 5);
        assert_eq!(pos, 4);
    }
}
