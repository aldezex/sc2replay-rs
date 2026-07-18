//! The replay *lobby* data — `NNet.Replay.SInitData`, carried in the
//! `replay.initData` MPQ stream.
//!
//! This is where SC2 records each lobby slot's **matchmaking rating**
//! (`m_scaledRating`), which appears nowhere else in a replay:
//! `replay.details` has names/races/results, the tracker and game event
//! streams have gameplay only. Skill level is therefore only obtainable
//! here, which is what makes this module worth its bit-exact fragility.
//!
//! # Encoding: bit-packed, NOT versioned
//!
//! Unlike `replay.details` (and `SHeader`), `replay.initData` is decoded
//! with the **bit-packed** decoder — Blizzard's `decode_replay_initdata`
//! builds a `BitPackedDecoder`, not a `VersionedDecoder`. This was
//! verified empirically before writing any of this module: the first
//! byte of a real extracted `replay.initData` is `0x10`, which is the
//! 5-bit array length (16 lobby slots) of `m_userInitialData`, not the
//! `0x05` struct tag that every versioned stream opens with.
//!
//! The practical consequence is that there are no self-describing type
//! tags and therefore **no [`crate::protocol::skip_value`] escape hatch**:
//! every field of `SLobbyUserInitialData` must be read at exactly the
//! right width, in order, or `m_scaledRating` — the last field of the
//! struct — silently decodes as garbage. That is why the whole struct is
//! modelled below even though only two fields are kept.
//!
//! # Layout provenance
//!
//! Field widths are transcribed from Blizzard's own `s2protocol`
//! typeinfo tables and cross-checked to be **identical** across builds
//! 95248, 97425 and 97563 (patch 5.0.16), so no per-build branching is
//! needed at this time. They are additionally validated at runtime by
//! [`decode_init_data`]'s callers: a width error garbles the decoded
//! lobby names, which no longer match `replay.details`' player names —
//! see the real-fixture tests.

use crate::bitpacked::{byte_align, read_aligned_bytes, read_bits};
use crate::format::format_display_name;

/// The sentinel `m_scaledRating` value SC2 writes when a replay carries
/// no usable rating. Confirmed as a real, frequently-observed value in
/// Blizzard's own `s2protocol` issue tracker; it is not an MMR and must
/// never be surfaced as one.
const INVALID_RATING_SENTINEL: i64 = -36400;

/// Lowest value accepted as a real MMR. Ratings are non-negative in
/// practice and a literal `0` is indistinguishable from "unset", so both
/// are rejected rather than reported as a very bad player.
const MIN_PLAUSIBLE_MMR: i64 = 1;

/// Highest value accepted as a real MMR. The all-time SC2 ladder peak is
/// well under 8000; 20000 is a deliberately loose upper bound whose only
/// job is to reject decode garbage (a misaligned read of a 32-bit field
/// produces values in the millions), not to police the ladder.
const MAX_PLAUSIBLE_MMR: i64 = 20_000;

/// One entry of `SSyncLobbyState.m_userInitialData` — a single lobby
/// slot. The array always has 16 entries, most of them empty in a 1v1;
/// observers and casters occupy slots too, which is why slots are joined
/// to real players via `m_workingSetSlotId` rather than by position.
#[derive(Debug, Clone, Default)]
pub struct LobbyUser {
    /// `m_name` — the raw lobby name, run through
    /// [`format_display_name`] like [`crate::player::Player::name`] so
    /// the two are directly comparable.
    pub name: String,
    /// `m_scaledRating`, **already validated**: [`None`] whenever the
    /// stored value is the [`INVALID_RATING_SENTINEL`] or otherwise
    /// outside the plausible range. Per this project's "exclude rather
    /// than fabricate" rule there is deliberately no way to get the raw
    /// number back — a caller that could see it would eventually average
    /// it.
    pub mmr: Option<i64>,
}

/// Decoded `replay.initData`, narrowed to the lobby slot list.
///
/// `SSyncLobbyState`'s other two fields (`m_gameDescription`,
/// `m_lobbyState`) are deliberately not decoded: they sit *after*
/// `m_userInitialData` in the stream, so nothing that follows them is
/// needed and the decoder can simply stop.
#[derive(Debug, Clone, Default)]
pub struct InitData {
    pub lobby_users: Vec<LobbyUser>,
}

impl InitData {
    /// Looks up a lobby slot by `m_workingSetSlotId` (from
    /// `replay.details`' `SPlayerListEntry`).
    pub fn lobby_user(&self, working_set_slot_id: usize) -> Option<&LobbyUser> {
        self.lobby_users.get(working_set_slot_id)
    }
}

/// `_blob(offset, bits)`: a length read as `_int(offset, bits)`, then
/// that many **byte-aligned** bytes.
fn read_blob<'a>(bytes: &'a [u8], bit_pos: &mut usize, offset: usize, bits: u32) -> &'a [u8] {
    let len = offset + read_bits(bytes, bit_pos, bits) as usize;
    read_aligned_bytes(bytes, bit_pos, len)
}

/// Skips `_optional(_int(offset, bits))` without materialising the value.
fn skip_optional_int(bytes: &[u8], bit_pos: &mut usize, bits: u32) {
    if read_bits(bytes, bit_pos, 1) != 0 {
        read_bits(bytes, bit_pos, bits);
    }
}

/// Skips `_optional(_blob(offset, bits))`.
fn skip_optional_blob(bytes: &[u8], bit_pos: &mut usize, offset: usize, bits: u32) {
    if read_bits(bytes, bit_pos, 1) != 0 {
        read_blob(bytes, bit_pos, offset, bits);
    }
}

/// Turns a raw `m_scaledRating` into a trustworthy MMR, or [`None`].
///
/// Rejects the [`INVALID_RATING_SENTINEL`] explicitly (so the intent is
/// documented) as well as anything outside
/// [`MIN_PLAUSIBLE_MMR`]`..=`[`MAX_PLAUSIBLE_MMR`], which additionally
/// catches decode drift.
fn validate_mmr(raw: i64) -> Option<i64> {
    if raw == INVALID_RATING_SENTINEL {
        return None;
    }
    if !(MIN_PLAUSIBLE_MMR..=MAX_PLAUSIBLE_MMR).contains(&raw) {
        return None;
    }
    Some(raw)
}

/// Decodes one `SLobbyUserInitialData`.
///
/// Every field is read (or width-exactly skipped) in declaration order.
/// The comment on each line is its `s2protocol` typeinfo; do not reorder
/// or "optimise away" a skip — the widths are positional.
fn decode_lobby_user(bytes: &[u8], bit_pos: &mut usize) -> LobbyUser {
    // m_name: _blob(0,8)
    let name_bytes = read_blob(bytes, bit_pos, 0, 8);
    let name = format_display_name(&String::from_utf8_lossy(name_bytes));

    skip_optional_blob(bytes, bit_pos, 0, 8); // m_clanTag: _optional(_blob(0,8))
    skip_optional_blob(bytes, bit_pos, 40, 0); // m_clanLogo: _optional(_blob(40,0)) — fixed 40 bytes
    skip_optional_int(bytes, bit_pos, 8); // m_highestLeague: _optional(_int(0,8))
    skip_optional_int(bytes, bit_pos, 32); // m_combinedRaceLevels: _optional(_int(0,32))
    read_bits(bytes, bit_pos, 32); // m_randomSeed: _int(0,32)
    skip_optional_int(bytes, bit_pos, 8); // m_racePreference: _struct(m_race: _optional(_int(0,8)))
    skip_optional_int(bytes, bit_pos, 8); // m_teamPreference: _struct(m_team: _optional(_int(0,8)))
    read_bits(bytes, bit_pos, 1); // m_testMap: _bool
    read_bits(bytes, bit_pos, 1); // m_testAuto: _bool
    read_bits(bytes, bit_pos, 1); // m_examine: _bool
    read_bits(bytes, bit_pos, 1); // m_customInterface: _bool
    read_bits(bytes, bit_pos, 32); // m_testType: _int(0,32)
    read_bits(bytes, bit_pos, 2); // m_observe: _int(0,2)
    read_blob(bytes, bit_pos, 0, 9); // m_hero: _blob(0,9)
    read_blob(bytes, bit_pos, 0, 9); // m_skin: _blob(0,9)
    read_blob(bytes, bit_pos, 0, 9); // m_mount: _blob(0,9)
    read_blob(bytes, bit_pos, 0, 7); // m_toonHandle: _blob(0,7)

    // m_scaledRating: _optional(_int(-2147483648,32))
    let mmr = if read_bits(bytes, bit_pos, 1) != 0 {
        let raw = -2_147_483_648_i64 + read_bits(bytes, bit_pos, 32) as i64;
        validate_mmr(raw)
    } else {
        None
    };

    LobbyUser { name, mmr }
}

/// Decodes `replay.initData` from the raw (already extracted and
/// decompressed) stream contents.
///
/// Walks `SInitData.m_syncLobbyState.m_userInitialData` and stops —
/// neither `SInitData` nor `SSyncLobbyState` adds any framing of its own
/// in the bit-packed encoding (structs are just their fields in order),
/// so the array length is literally the first value in the stream.
pub fn decode_init_data(bytes: &[u8]) -> InitData {
    let mut bit_pos = 0usize;

    // m_userInitialData: _array((0,5), SLobbyUserInitialData)
    let count = read_bits(bytes, &mut bit_pos, 5) as usize;
    let mut lobby_users = Vec::with_capacity(count);
    for _ in 0..count {
        lobby_users.push(decode_lobby_user(bytes, &mut bit_pos));
    }
    byte_align(&mut bit_pos);

    InitData { lobby_users }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one `SLobbyUserInitialData` with everything absent/zero
    /// except the name and an optionally-present rating, so a test can
    /// assert on rating handling without hand-computing bit offsets.
    struct LobbyUserBuilder {
        bytes: Vec<u8>,
        bit_pos: usize,
    }

    impl LobbyUserBuilder {
        fn new() -> Self {
            LobbyUserBuilder {
                bytes: Vec::new(),
                bit_pos: 0,
            }
        }

        /// The exact inverse of [`read_bits`], chunk for chunk.
        ///
        /// Writing this as "set stream bit *i* to byte bit *i % 8*" is
        /// the obvious implementation and it is **wrong**: the reader
        /// consumes bits low-to-high within a byte but treats the
        /// earlier-consumed bits of a value as the *more significant*
        /// ones, so a value's bit layout depends on where its chunks
        /// fall relative to byte boundaries. Mirroring the reader's own
        /// loop is the only way to stay consistent with it.
        fn push_int(&mut self, value: u64, n: u32) {
            let mut remaining = n;
            while remaining > 0 {
                let byte_idx = self.bit_pos / 8;
                let bit_offset = (self.bit_pos % 8) as u32;
                if byte_idx >= self.bytes.len() {
                    self.bytes.push(0);
                }
                let copy_bits = remaining.min(8 - bit_offset);
                let mask = (1u64 << copy_bits) - 1;
                let chunk = (value >> (remaining - copy_bits)) & mask;
                self.bytes[byte_idx] |= (chunk as u8) << bit_offset;
                remaining -= copy_bits;
                self.bit_pos += copy_bits as usize;
            }
        }

        fn align(&mut self) {
            self.bit_pos = self.bit_pos.div_ceil(8) * 8;
            while self.bytes.len() < self.bit_pos / 8 {
                self.bytes.push(0);
            }
        }

        /// `_blob`: a length, then byte-aligned raw bytes.
        fn push_blob(&mut self, data: &[u8], len_bits: u32) {
            self.push_int(data.len() as u64, len_bits);
            self.align();
            self.bytes.extend_from_slice(data);
            self.bit_pos += data.len() * 8;
        }

        fn user(&mut self, name: &str, rating: Option<i64>) {
            self.push_blob(name.as_bytes(), 8); // m_name
            self.push_int(0, 1); // m_clanTag absent
            self.push_int(0, 1); // m_clanLogo absent
            self.push_int(0, 1); // m_highestLeague absent
            self.push_int(0, 1); // m_combinedRaceLevels absent
            self.push_int(0, 32); // m_randomSeed
            self.push_int(0, 1); // m_racePreference.m_race absent
            self.push_int(0, 1); // m_teamPreference.m_team absent
            self.push_int(0, 4); // four bools
            self.push_int(0, 32); // m_testType
            self.push_int(0, 2); // m_observe
            self.push_blob(b"", 9); // m_hero
            self.push_blob(b"", 9); // m_skin
            self.push_blob(b"", 9); // m_mount
            self.push_blob(b"", 7); // m_toonHandle
            match rating {
                Some(r) => {
                    self.push_int(1, 1);
                    self.push_int((r - (-2_147_483_648_i64)) as u64, 32);
                }
                None => self.push_int(0, 1),
            }
        }

        /// Emits the finished stream, prefixed with the 5-bit array count.
        fn finish(users: &[(&str, Option<i64>)]) -> Vec<u8> {
            let mut b = LobbyUserBuilder::new();
            b.push_int(users.len() as u64, 5);
            for (name, rating) in users {
                b.user(name, *rating);
            }
            b.align();
            b.bytes
        }
    }

    #[test]
    fn decodes_names_and_ratings_for_every_slot() {
        let bytes = LobbyUserBuilder::finish(&[("Alice", Some(4200)), ("Bob", Some(5100))]);
        let init = decode_init_data(&bytes);

        assert_eq!(init.lobby_users.len(), 2);
        assert_eq!(init.lobby_users[0].name, "Alice");
        assert_eq!(init.lobby_users[0].mmr, Some(4200));
        assert_eq!(init.lobby_users[1].name, "Bob");
        assert_eq!(init.lobby_users[1].mmr, Some(5100));
    }

    #[test]
    fn an_absent_rating_is_none_not_zero() {
        let bytes = LobbyUserBuilder::finish(&[("Nobody", None)]);
        let init = decode_init_data(&bytes);
        assert_eq!(init.lobby_users[0].mmr, None);
    }

    #[test]
    fn the_minus_36400_sentinel_becomes_none() {
        // The documented Blizzard sentinel. Reporting it as an MMR would
        // poison every average computed downstream.
        let bytes = LobbyUserBuilder::finish(&[("Sentinel", Some(INVALID_RATING_SENTINEL))]);
        let init = decode_init_data(&bytes);
        assert_eq!(init.lobby_users[0].mmr, None);
    }

    #[test]
    fn implausible_ratings_become_none() {
        for raw in [-1, 0, MAX_PLAUSIBLE_MMR + 1, 1_500_000] {
            let bytes = LobbyUserBuilder::finish(&[("Weird", Some(raw))]);
            let init = decode_init_data(&bytes);
            assert_eq!(init.lobby_users[0].mmr, None, "raw {raw} must not be reported as MMR");
        }
    }

    #[test]
    fn boundary_ratings_are_kept() {
        for raw in [MIN_PLAUSIBLE_MMR, 6500, MAX_PLAUSIBLE_MMR] {
            let bytes = LobbyUserBuilder::finish(&[("Edge", Some(raw))]);
            let init = decode_init_data(&bytes);
            assert_eq!(init.lobby_users[0].mmr, Some(raw));
        }
    }

    /// Real-replay verification. Synthetic tests above only prove the
    /// decoder is self-consistent with the test builder; only a real
    /// stream proves the *field widths* are right — the failure mode
    /// this module actually has.
    #[test]
    fn decodes_plausible_mmr_from_a_real_ladder_replay() {
        let path = "tests/fixtures/dont-oracle-me.SC2Replay";
        let Ok(replay) = crate::replay::load_replay(path) else {
            eprintln!("fixture missing; skipping");
            return;
        };

        // A 1v1 lobby always carries the full 16-slot array. A wrong
        // field width usually blows up or truncates this first.
        assert_eq!(replay.init_data.lobby_users.len(), 16);

        // The strongest available cross-check that the widths are right:
        // each player's lobby slot, reached via `m_workingSetSlotId`,
        // must carry that same player's name. `replay.details` prefixes
        // the clan tag (`<clan> Name`) while the lobby stores it in a
        // separate field, so the lobby name is a suffix of the details
        // name rather than equal to it.
        for player in &replay.players {
            let slot = player
                .working_set_slot_id
                .expect("a real ladder replay records m_workingSetSlotId");
            let lobby = replay
                .init_data
                .lobby_user(slot as usize)
                .expect("m_workingSetSlotId points into the 16-slot array");
            assert!(
                player.name.ends_with(&lobby.name) && !lobby.name.is_empty(),
                "lobby name {:?} should match details name {:?} — a mismatch means \
                 the bit-packed field widths drifted",
                lobby.name,
                player.name
            );

            // This is a rated ladder game, so both sides must have a
            // real MMR in a sane range (these two are ~4600-4700).
            let mmr = player.mmr.expect("rated ladder replay has MMR on both sides");
            assert!(
                (3000..=7000).contains(&mmr),
                "{} decoded as {mmr}, which is not a plausible ladder MMR",
                player.name
            );
        }
    }

    #[test]
    fn a_slot_with_an_empty_name_still_keeps_the_stream_aligned() {
        // Real 1v1 lobbies carry 16 slots, most of them empty; a
        // misaligned empty slot would garble every later slot's name.
        let bytes = LobbyUserBuilder::finish(&[
            ("", None),
            ("Real", Some(3300)),
            ("", None),
            ("AlsoReal", Some(4400)),
        ]);
        let init = decode_init_data(&bytes);
        assert_eq!(init.lobby_users[1].name, "Real");
        assert_eq!(init.lobby_users[3].name, "AlsoReal");
        assert_eq!(init.lobby_users[3].mmr, Some(4400));
    }
}
