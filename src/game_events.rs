//! Decoding of `replay.game.events`: player-issued commands (unit
//! training, building placement, move/attack orders, ability use).
//!
//! Only `NNet.Game.SCmdEvent` (event id 27, typeid 100 in `protocol97425`)
//! is modeled. Every other `NNet.Game.*Event` type is treated as
//! unsupported and causes [`decode_game_events`] to return
//! [`GameEventsError::UnsupportedEventId`] rather than being generically
//! skipped: unlike `VersionedDecoder`, `BitPackedDecoder` has no type
//! tags, so a value of unknown type cannot be skipped without knowing its
//! exact bit layout ahead of time — guessing risks silently misaligning
//! the rest of the stream. See the crate README for the resulting
//! limitations.
//!
//! Field layout cross-checked directly against `protocol97425.py`'s
//! `typeinfos` (indices 0, 2, 6, 8, 10, 25, 43, 47, 60, 83, 91-100).

use crate::bitpacked::{byte_align, read_int, read_optional, read_optional_int, read_var_uint32};

/// A decoded `replay.game.events` entry. Only one variant exists today
/// (`Cmd`); the enum shape leaves room for future `NNet.Game.*Event`
/// types without an API break.
#[derive(Debug, Clone)]
pub enum GameEvent {
    /// `NNet.Game.SCmdEvent` (event id 27).
    Cmd(CmdEvent),
}

/// A player-issued command (`SCmdEvent`, typeid 100).
///
/// The field most relevant to build-order reconstruction is [`CmdEvent::abil`]:
/// `abil_link` + `abil_cmd_index` identify *which* ability/command was
/// issued (e.g. "train SCV"), but only as a numeric id — mapping that id
/// to a human-readable name requires a `CommandCard`/ability data table
/// not present in `protocol97425.py`, and is out of scope here; callers
/// get the raw ids.
#[derive(Debug, Clone)]
pub struct CmdEvent {
    pub gameloop: i64,
    pub user_id: i64,
    pub cmd_flags: i64,
    pub abil: Option<AbilCmd>,
    pub data: CmdData,
    pub sequence: i64,
    pub other_unit: Option<i64>,
    pub unit_group: Option<i64>,
}

/// Which ability/command was issued (`m_abil`, typeid 92/93).
#[derive(Debug, Clone)]
pub struct AbilCmd {
    pub abil_link: i64,
    pub abil_cmd_index: i64,
    pub abil_cmd_data: Option<i64>,
}

/// A 3D game-world coordinate (typeid 96/47), shared by [`CmdData::TargetPoint`]
/// and [`TargetUnit::snapshot_point`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point3 {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

/// The command's target, if any (`m_data`, typeid 98 — a 4-way choice).
#[derive(Debug, Clone)]
pub enum CmdData {
    None,
    TargetPoint(Point3),
    TargetUnit(TargetUnit),
    Data(i64),
}

/// `m_data`'s `TargetUnit` variant (typeid 97).
#[derive(Debug, Clone)]
pub struct TargetUnit {
    pub target_unit_flags: i64,
    pub timer: i64,
    pub tag: i64,
    pub snapshot_unit_link: i64,
    pub snapshot_control_player_id: Option<i64>,
    pub snapshot_upkeep_player_id: Option<i64>,
    pub snapshot_point: Point3,
}

/// Errors decoding `replay.game.events`.
#[derive(Debug, thiserror::Error)]
pub enum GameEventsError {
    /// An event id whose typeid isn't `SCmdEvent`'s. Since this format
    /// has no generic "skip a value of unknown type" (no tags), decoding
    /// stops rather than risk silently misaligning the rest of the
    /// stream.
    #[error("unsupported game event id {event_id} at bit position {bit_pos} (not SCmdEvent)")]
    UnsupportedEventId { event_id: i64, bit_pos: usize },
}

fn decode_point3(bytes: &[u8], bit_pos: &mut usize) -> Point3 {
    Point3 {
        x: read_int(bytes, bit_pos, 0, 20),
        y: read_int(bytes, bit_pos, 0, 20),
        z: read_int(bytes, bit_pos, -2147483648, 32),
    }
}

fn decode_abil_cmd(bytes: &[u8], bit_pos: &mut usize) -> AbilCmd {
    AbilCmd {
        abil_link: read_int(bytes, bit_pos, 0, 16),
        abil_cmd_index: read_int(bytes, bit_pos, 0, 5),
        abil_cmd_data: read_optional_int(bytes, bit_pos, 0, 8),
    }
}

fn decode_target_unit(bytes: &[u8], bit_pos: &mut usize) -> TargetUnit {
    TargetUnit {
        target_unit_flags: read_int(bytes, bit_pos, 0, 16),
        timer: read_int(bytes, bit_pos, 0, 8),
        tag: read_int(bytes, bit_pos, 0, 32),
        snapshot_unit_link: read_int(bytes, bit_pos, 0, 16),
        snapshot_control_player_id: read_optional_int(bytes, bit_pos, 0, 4),
        snapshot_upkeep_player_id: read_optional_int(bytes, bit_pos, 0, 4),
        snapshot_point: decode_point3(bytes, bit_pos),
    }
}

fn decode_cmd_data(bytes: &[u8], bit_pos: &mut usize) -> CmdData {
    match read_int(bytes, bit_pos, 0, 2) {
        0 => CmdData::None,
        1 => CmdData::TargetPoint(decode_point3(bytes, bit_pos)),
        2 => CmdData::TargetUnit(decode_target_unit(bytes, bit_pos)),
        3 => CmdData::Data(read_int(bytes, bit_pos, 0, 32)),
        _ => unreachable!("2-bit selector is always 0..=3"),
    }
}

/// Decodes an `SCmdEvent` body (typeid 100), fields read positionally
/// per `protocol97425.py`'s `typeinfos[100]`.
fn decode_cmd_event(bytes: &[u8], bit_pos: &mut usize, gameloop: i64, user_id: i64) -> CmdEvent {
    CmdEvent {
        gameloop,
        user_id,
        cmd_flags: read_int(bytes, bit_pos, 0, 27),
        abil: read_optional(bytes, bit_pos, decode_abil_cmd),
        data: decode_cmd_data(bytes, bit_pos),
        sequence: read_int(bytes, bit_pos, 1, 32),
        other_unit: read_optional_int(bytes, bit_pos, 0, 32),
        unit_group: read_optional_int(bytes, bit_pos, 0, 32),
    }
}

/// Decodes the full `replay.game.events` stream into a list of
/// [`GameEvent`]s, in chronological order.
///
/// Each event is prefixed by a gameloop delta ([`read_var_uint32`]) and a
/// user id (`replay_userid_typeid`, typeid 8 -> typeid 2: `_int(0,5)`),
/// then a 7-bit event id (`game_eventid_typeid`, typeid 0). Event id 27 is
/// `SCmdEvent`; any other id is unsupported and returns
/// [`GameEventsError::UnsupportedEventId`] (see the module doc for why
/// this can't degrade gracefully the way
/// [`crate::events::decode_tracker_events`]'s `Unknown` fallback does).
/// After each event, the stream re-aligns to the next byte boundary.
///
/// # Errors
/// Returns [`GameEventsError::UnsupportedEventId`] on the first event id
/// other than 27.
pub fn decode_game_events(bytes: &[u8]) -> Result<Vec<GameEvent>, GameEventsError> {
    let mut bit_pos = 0usize;
    let mut gameloop: i64 = 0;
    let total_bits = bytes.len() * 8;
    let mut events = Vec::new();

    while bit_pos < total_bits {
        let delta = read_var_uint32(bytes, &mut bit_pos);
        gameloop += delta;

        let user_id = read_int(bytes, &mut bit_pos, 0, 5);
        let event_id = read_int(bytes, &mut bit_pos, 0, 7);

        let event = match event_id {
            27 => GameEvent::Cmd(decode_cmd_event(bytes, &mut bit_pos, gameloop, user_id)),
            other => {
                return Err(GameEventsError::UnsupportedEventId {
                    event_id: other,
                    bit_pos,
                });
            }
        };

        events.push(event);
        byte_align(&mut bit_pos);
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_minimal_all_zero_cmd_event() {
        // Every field is either a zero-valued fixed-width int or an
        // absent optional (presence bit 0); m_sequence has offset 1, so
        // its raw zero bits decode to 1. Total width is exactly 64 bits
        // (8 bytes): 27 (cmd_flags) + 1 (abil presence) + 2 (data
        // selector) + 32 (sequence) + 1 (other_unit presence) +
        // 1 (unit_group presence). This only validates field-width
        // bookkeeping, not bit order (all-zero input is order-independent
        // — bit order is covered by bitpacked.rs's tests).
        let bytes = [0u8; 8];
        let mut bit_pos = 0;

        let event = decode_cmd_event(&bytes, &mut bit_pos, 42, 3);

        assert_eq!(event.gameloop, 42);
        assert_eq!(event.user_id, 3);
        assert_eq!(event.cmd_flags, 0);
        assert!(event.abil.is_none());
        assert!(matches!(event.data, CmdData::None));
        assert_eq!(event.sequence, 1);
        assert!(event.other_unit.is_none());
        assert!(event.unit_group.is_none());
        assert_eq!(bit_pos, 64);
    }
}
