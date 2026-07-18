//! The replay *header* — the versioned `NNet.Replay.SHeader` structure
//! carried in the MPQ user-data block at the very start of a `.SC2Replay`
//! file, before the real MPQ archive.
//!
//! [`crate::replay::load_replay`] normally jumps straight past this block
//! (via `MpqUserDataHeader.header_offset`) to the archive proper. This
//! module reads the version/build fields the header carries, so consumers
//! can tell which game build a replay was recorded on — e.g. to
//! distinguish patch 5.0.16 / build 97563 from earlier builds when a
//! balance patch changes constants that analysis depends on.

use crate::protocol::{read_struct, read_tagged_int, skip_value};

/// The SC2 game version/build a replay was recorded on, from
/// `SHeader.m_version`.
///
/// `base_build` is the build whose protocol definition applies (what
/// s2protocol keys its versioned decoder on); `build` is the exact client
/// build. They are usually equal but need not be. `major.minor.revision`
/// is the human-facing version (e.g. `5.0.16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplayVersion {
    pub major: i64,
    pub minor: i64,
    pub revision: i64,
    pub build: i64,
    pub base_build: i64,
}

/// The versioned `SHeader` struct begins immediately after the 16-byte
/// MPQ user-data header (`MPQ\x1B` magic + three `u32` fields:
/// `user_data_size`, `header_offset`, `user_data_header_size`). The
/// header content is stored raw — no decompression needed.
const USER_DATA_CONTENT_OFFSET: usize = 16;

/// Decodes [`ReplayVersion`] straight from the raw replay-file bytes.
///
/// Reads `SHeader.m_version` (field 1) — a nested struct whose fields
/// `m_major` / `m_minor` / `m_revision` / `m_build` / `m_baseBuild`
/// (indices 1..=5) are plain ints — and skips every other `SHeader`
/// field with [`skip_value`] to stay aligned without modelling them.
///
/// Empirically verified against real replays on both build 97425 (this
/// crate's reference build) and build 97563 (patch 5.0.16) — the versioned
/// encoding is self-describing, so this does not depend on the protocol
/// version the surrounding decoder was generated from.
pub fn decode_replay_version(bytes: &[u8]) -> ReplayVersion {
    let mut pos = USER_DATA_CONTENT_OFFSET;
    let mut version = ReplayVersion::default();

    read_struct(bytes, &mut pos, |b, p, field_index| {
        if field_index == 1 {
            read_struct(b, p, |b2, p2, version_field| match version_field {
                1 => version.major = read_tagged_int(b2, p2),
                2 => version.minor = read_tagged_int(b2, p2),
                3 => version.revision = read_tagged_int(b2, p2),
                4 => version.build = read_tagged_int(b2, p2),
                5 => version.base_build = read_tagged_int(b2, p2),
                _ => skip_value(b2, p2).unwrap(),
            });
        } else {
            skip_value(b, p).unwrap();
        }
    });

    version
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tagged vint (type `0x09`) holding a small non-negative value
    /// (fits the single-byte form: sign bit 0, value in bits 1..=6).
    fn tagged_small_int(v: u8) -> [u8; 2] {
        [0x09, v << 1]
    }

    #[test]
    fn decodes_version_from_synthetic_header() {
        let mut bytes = vec![0u8; USER_DATA_CONTENT_OFFSET]; // MPQ user-data header padding
        // SHeader struct: one field, index 1 = m_version.
        bytes.push(0x05); // struct
        bytes.push(0x02); // num_fields = vint(1)
        bytes.push(0x02); // field_index = vint(1)
        // m_version struct: fields 1..=5.
        bytes.push(0x05); // struct
        bytes.push(0x0A); // num_fields = vint(5)
        for (field_index, value) in [(1u8, 5u8), (2, 0), (3, 16), (4, 50), (5, 42)] {
            bytes.push(field_index << 1); // field_index vint
            bytes.extend_from_slice(&tagged_small_int(value));
        }

        let v = decode_replay_version(&bytes);
        assert_eq!(
            v,
            ReplayVersion {
                major: 5,
                minor: 0,
                revision: 16,
                build: 50,
                base_build: 42,
            }
        );
    }
}
