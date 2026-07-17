//! Decoding of `replay.game.events`: player-issued commands (unit
//! training, building placement, move/attack orders, ability use) and
//! unit-selection state changes.
//!
//! `NNet.Game.SCmdEvent` (event id 27, typeid 100), `SSelectionDeltaEvent`
//! (event id 28, typeid 109), and `SControlGroupUpdateEvent` (event id 29,
//! typeid 110) are fully modeled with named fields. Every other
//! `NNet.Game.*Event` type is generically **skipped** rather than
//! modeled: unlike `VersionedDecoder`, `BitPackedDecoder` has no type
//! tags, so a value of unknown type can't be skipped by a simple
//! recursive "skip whatever tag you find" the way
//! [`crate::protocol::skip_value`] does. Instead, [`skip_bitpacked_value`]
//! looks up the event's typeid's structure in [`crate::typeinfos`] (a
//! Rust transcription of `protocol97425.py`'s `typeinfos` table) and
//! computes exactly how many bits it occupies, recursively, without
//! needing to interpret its contents.
//!
//! An event id that isn't present in `game_event_types` at all (as
//! opposed to one that's present but unmodeled) is a genuinely unknown
//! id and causes [`decode_game_events`] to return
//! [`GameEventsError::UnsupportedEventId`] — this should not happen for
//! any normal replay, since [`typeid_for_event`] is a complete
//! transcription of `protocol97425.py`'s `game_event_types` dispatch
//! table.
//!
//! Field layout cross-checked directly against `protocol97425.py`'s
//! `typeinfos` (indices 0, 2, 6, 8, 10, 25, 43, 47, 60, 83, 91-110), all
//! fetched from the reference source directly (not hand-transcribed from
//! memory) — see each decode function's doc comment for the specific
//! field names confirmed.

use crate::bitpacked::{
    byte_align, read_aligned_bytes, read_bits, read_int, read_optional, read_optional_int,
    read_var_uint32,
};
use crate::typeinfos::{TypeInfo, typeinfo};

/// A decoded `replay.game.events` entry. Three variants are fully
/// modeled (`Cmd`, `SelectionDelta`, `ControlGroupUpdate`); the enum
/// shape leaves room for more `NNet.Game.*Event` types without an API
/// break — **`match`ing this exhaustively without a wildcard arm is not
/// guaranteed to keep compiling across versions of this crate.**
#[derive(Debug, Clone)]
pub enum GameEvent {
    /// `NNet.Game.SCmdEvent` (event id 27, typeid 100).
    Cmd(CmdEvent),
    /// `NNet.Game.SSelectionDeltaEvent` (event id 28, typeid 109): a
    /// player's current unit selection changed (click, drag-select, or
    /// a control-group recall).
    SelectionDelta(SelectionDeltaEvent),
    /// `NNet.Game.SControlGroupUpdateEvent` (event id 29, typeid 110): a
    /// player set/added-to/recalled a numbered control group (hotkey).
    ControlGroupUpdate(ControlGroupUpdateEvent),
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

/// `m_removeMask`/`m_mask` (typeid 104): a `_choice` describing which of
/// a *previous* selection's units are affected, encoded whichever way
/// is more compact for the given selection size. Confirmed verbatim
/// against `protocol97425.py`: `_choice([(0,2),{0:('None',94),
/// 1:('Mask',102),2:('OneIndices',103),3:('ZeroIndices',103)}])`.
///
/// `Mask`'s bits are in *subgroup order* (the order units appear in the
/// selection's flattened subgroup list, not by tag) — this crate does
/// not reconstruct that order, so `Mask`/`OneIndices`/`ZeroIndices` are
/// exposed as raw index data for callers that want to attempt it, not
/// resolved to unit tags here.
#[derive(Debug, Clone)]
pub enum SelectionMask {
    None,
    /// A bitarray of `len` bits (typeid 102: `_bitarray(bound=(0,9))`),
    /// one per previously-selected unit in subgroup order. The raw bits
    /// aren't captured here — a selection can exceed 64 units, more
    /// than fits in one machine word — only the length is exposed,
    /// consistent with this type's "not resolved to unit tags" scope
    /// note above.
    Mask { len: u32 },
    /// Indices (into the previous selection's subgroup order) whose bit
    /// is 1.
    OneIndices(Vec<i64>),
    /// Indices (into the previous selection's subgroup order) whose bit
    /// is 0.
    ZeroIndices(Vec<i64>),
}

/// One entry of `m_addSubgroups` (typeid 105): a batch of newly-added
/// units sharing the same unit type/priority, added to the current
/// selection. Confirmed verbatim against `protocol97425.py`:
/// `_struct([('m_unitLink',83,-4),('m_subgroupPriority',10,-3),
/// ('m_intraSubgroupPriority',10,-2),('m_count',101,-1)])`.
#[derive(Debug, Clone, Copy)]
pub struct AddSubgroup {
    pub unit_link: i64,
    pub subgroup_priority: i64,
    pub intra_subgroup_priority: i64,
    pub count: i64,
}

/// `NNet.Game.SSelectionDeltaEvent` (event id 28, typeid 109): a
/// player's current unit selection changed. Confirmed verbatim against
/// `protocol97425.py`: typeid 109 is `_struct([('m_controlGroupId',1,-7),
/// ('m_delta',108,-6)])`, and typeid 108 (`m_delta`) is
/// `_struct([('m_subgroupIndex',101,-4),('m_removeMask',104,-3),
/// ('m_addSubgroups',106,-2),('m_addUnitTags',107,-1)])`.
///
/// [`Self::add_unit_tags`] is the field most relevant to callers: it
/// directly lists the raw unit tags (same `tag` encoding as
/// [`TargetUnit::tag`]) newly added to the selection — e.g. by clicking
/// a production structure. This crate does not attempt to reconstruct
/// the *full* current selection (that requires tracking subgroup order
/// across events to resolve [`SelectionMask`] removals to specific
/// tags) — see the module doc's scope note.
#[derive(Debug, Clone)]
pub struct SelectionDeltaEvent {
    pub gameloop: i64,
    pub user_id: i64,
    pub control_group_id: i64,
    pub subgroup_index: i64,
    pub remove_mask: SelectionMask,
    pub add_subgroups: Vec<AddSubgroup>,
    pub add_unit_tags: Vec<i64>,
}

/// `NNet.Game.SControlGroupUpdateEvent` (event id 29, typeid 110): a
/// player set/added-to/recalled a numbered control group (hotkey).
/// Confirmed verbatim against `protocol97425.py`:
/// `_struct([('m_controlGroupIndex',1,-8),('m_controlGroupUpdate',12,-7),
/// ('m_mask',104,-6)])`.
///
/// [`Self::control_group_update`]'s meaning (cross-checked against the
/// `sc2reader` Python project's `create_control_group_event`, since
/// `protocol97425.py` itself doesn't name the enum values): `0` = Set
/// (replace the group's contents with the current selection), `1` = Add
/// (add the current selection to the group), `2` = Get/Recall (replace
/// the current selection with the group's contents), `3` = a rarer
/// "steal" variant whose exact semantics aren't pinned down by either
/// source — exposed as a raw `i64`, not an enum, so callers can decide
/// how to handle `3` themselves.
#[derive(Debug, Clone)]
pub struct ControlGroupUpdateEvent {
    pub gameloop: i64,
    pub user_id: i64,
    pub control_group_index: i64,
    pub control_group_update: i64,
    pub mask: SelectionMask,
}

/// Errors decoding `replay.game.events`.
#[derive(Debug, thiserror::Error)]
pub enum GameEventsError {
    /// An event id not present in `game_event_types` at all — a
    /// genuinely unknown id, not just an unmodeled-but-skippable one.
    /// Should not happen for any normal replay; see the module doc.
    #[error("unsupported game event id {event_id} at bit position {bit_pos} (not in game_event_types)")]
    UnsupportedEventId { event_id: i64, bit_pos: usize },
    /// A `_choice` value's selector didn't match any of its known
    /// variants. Since a choice's variant determines what (and how much)
    /// comes next, this makes it impossible to keep skipping correctly,
    /// so decoding stops here rather than risk misaligning the rest of
    /// the stream.
    #[error(
        "typeid {typeid} choice selector {selector} at bit position {bit_pos} matches no known variant"
    )]
    UnknownChoiceVariant {
        typeid: usize,
        selector: i64,
        bit_pos: usize,
    },
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

/// Decodes an `m_removeMask`/`m_mask` value (typeid 104).
fn decode_selection_mask(bytes: &[u8], bit_pos: &mut usize) -> SelectionMask {
    match read_int(bytes, bit_pos, 0, 2) {
        0 => SelectionMask::None,
        1 => {
            // typeid 102: _bitarray(bound=(0,9)) -- a 9-bit length
            // prefix, then that many further (unaligned) bits, same
            // shape as skip_bitpacked_value's own BitArray handling.
            let len = read_int(bytes, bit_pos, 0, 9) as u32;
            let _ = read_bits(bytes, bit_pos, len);
            SelectionMask::Mask { len }
        }
        2 => SelectionMask::OneIndices(read_int9_array(bytes, bit_pos)),
        3 => SelectionMask::ZeroIndices(read_int9_array(bytes, bit_pos)),
        _ => unreachable!("2-bit selector is always 0..=3"),
    }
}

/// `_array(bound=(0,9), element=_int(0,9))` — the shape shared by
/// `OneIndices`/`ZeroIndices` (typeid 103).
fn read_int9_array(bytes: &[u8], bit_pos: &mut usize) -> Vec<i64> {
    let count = read_int(bytes, bit_pos, 0, 9);
    (0..count).map(|_| read_int(bytes, bit_pos, 0, 9)).collect()
}

/// Decodes one `m_addSubgroups` entry (typeid 105).
fn decode_add_subgroup(bytes: &[u8], bit_pos: &mut usize) -> AddSubgroup {
    AddSubgroup {
        unit_link: read_int(bytes, bit_pos, 0, 16),
        subgroup_priority: read_int(bytes, bit_pos, 0, 8),
        intra_subgroup_priority: read_int(bytes, bit_pos, 0, 8),
        count: read_int(bytes, bit_pos, 0, 9),
    }
}

/// Decodes an `SSelectionDeltaEvent` body (typeid 109), fields read
/// positionally per `protocol97425.py`'s `typeinfos[109]`/`typeinfos[108]`.
fn decode_selection_delta_event(
    bytes: &[u8],
    bit_pos: &mut usize,
    gameloop: i64,
    user_id: i64,
) -> SelectionDeltaEvent {
    let control_group_id = read_int(bytes, bit_pos, 0, 4);
    let subgroup_index = read_int(bytes, bit_pos, 0, 9);
    let remove_mask = decode_selection_mask(bytes, bit_pos);
    let add_subgroups_count = read_int(bytes, bit_pos, 0, 9);
    let add_subgroups = (0..add_subgroups_count)
        .map(|_| decode_add_subgroup(bytes, bit_pos))
        .collect();
    let add_unit_tags_count = read_int(bytes, bit_pos, 0, 9);
    let add_unit_tags = (0..add_unit_tags_count)
        .map(|_| read_int(bytes, bit_pos, 0, 32))
        .collect();

    SelectionDeltaEvent {
        gameloop,
        user_id,
        control_group_id,
        subgroup_index,
        remove_mask,
        add_subgroups,
        add_unit_tags,
    }
}

/// Decodes an `SControlGroupUpdateEvent` body (typeid 110), fields read
/// positionally per `protocol97425.py`'s `typeinfos[110]`.
fn decode_control_group_update_event(
    bytes: &[u8],
    bit_pos: &mut usize,
    gameloop: i64,
    user_id: i64,
) -> ControlGroupUpdateEvent {
    ControlGroupUpdateEvent {
        gameloop,
        user_id,
        control_group_index: read_int(bytes, bit_pos, 0, 4),
        control_group_update: read_int(bytes, bit_pos, 0, 3),
        mask: decode_selection_mask(bytes, bit_pos),
    }
}

/// Skips a value of the given `typeid` without interpreting its
/// contents, by recursively looking up its structure in
/// [`crate::typeinfos`] and consuming exactly the right number of bits.
///
/// Unlike [`crate::protocol::skip_value`] (which works off a runtime
/// type tag), this is not a blind byte-count skip: `_optional`,
/// `_choice`, `_array`, `_blob`, and `_bitarray` all need to actually
/// decode a small piece of data (a presence bit, a selector, a count, a
/// length) to know how much more there is to skip — it's "decode and
/// discard the semantic value" more than "skip N known bytes".
///
/// # Errors
/// Returns [`GameEventsError::UnknownChoiceVariant`] if a `_choice`'s
/// selector doesn't match any of its defined variants — this shouldn't
/// happen for any value actually produced by a real SC2 client, but is
/// handled explicitly rather than risking a silent misalignment of the
/// rest of the stream.
fn skip_bitpacked_value(
    bytes: &[u8],
    bit_pos: &mut usize,
    typeid: usize,
) -> Result<(), GameEventsError> {
    match typeinfo(typeid) {
        TypeInfo::Int { bits } => {
            read_bits_and_discard(bytes, bit_pos, bits);
        }
        TypeInfo::Bool => {
            read_bits_and_discard(bytes, bit_pos, 1);
        }
        TypeInfo::Optional { inner } => {
            if read_int(bytes, bit_pos, 0, 1) != 0 {
                skip_bitpacked_value(bytes, bit_pos, inner)?;
            }
        }
        TypeInfo::Choice {
            bound_offset,
            bound_bits,
            variants,
        } => {
            let selector = read_int(bytes, bit_pos, bound_offset, bound_bits);
            let variant_typeid = variants
                .iter()
                .find(|(sel, _)| *sel == selector)
                .map(|&(_, variant_typeid)| variant_typeid)
                .ok_or(GameEventsError::UnknownChoiceVariant {
                    typeid,
                    selector,
                    bit_pos: *bit_pos,
                })?;
            skip_bitpacked_value(bytes, bit_pos, variant_typeid)?;
        }
        TypeInfo::Struct { fields } => {
            for &field_typeid in fields {
                skip_bitpacked_value(bytes, bit_pos, field_typeid)?;
            }
        }
        TypeInfo::Array {
            bound_offset,
            bound_bits,
            element,
        } => {
            let count = read_int(bytes, bit_pos, bound_offset, bound_bits);
            for _ in 0..count {
                skip_bitpacked_value(bytes, bit_pos, element)?;
            }
        }
        TypeInfo::Blob {
            bound_offset,
            bound_bits,
        } => {
            let len = read_int(bytes, bit_pos, bound_offset, bound_bits) as usize;
            read_aligned_bytes(bytes, bit_pos, len);
        }
        TypeInfo::FourCc => {
            read_bits_and_discard(bytes, bit_pos, 32);
        }
        TypeInfo::Null => {}
        TypeInfo::BitArray {
            bound_offset,
            bound_bits,
        } => {
            let len = read_int(bytes, bit_pos, bound_offset, bound_bits) as u32;
            read_bits_and_discard(bytes, bit_pos, len);
        }
    }

    Ok(())
}

/// `read_int`, but for callers that only care about advancing `bit_pos`
/// and not the decoded value.
fn read_bits_and_discard(bytes: &[u8], bit_pos: &mut usize, bits: u32) {
    let _ = read_int(bytes, bit_pos, 0, bits);
}

/// Looks up the typeid for a `replay.game.events` event id, per
/// `protocol97425.py`'s `game_event_types` table. Returns `None` for an
/// id genuinely absent from that table (as opposed to one that's present
/// but unmodeled, which is handled by [`skip_bitpacked_value`] instead).
fn typeid_for_event(event_id: i64) -> Option<usize> {
    match event_id {
        5 => Some(82),
        7 => Some(81),
        9 => Some(74),
        10 => Some(76),
        11 => Some(77),
        12 => Some(78),
        13 => Some(80),
        14 => Some(85),
        21 => Some(86),
        22 => Some(82),
        23 => Some(82),
        25 => Some(87),
        26 => Some(90),
        27 => Some(100),
        28 => Some(109),
        29 => Some(110),
        30 => Some(112),
        31 => Some(114),
        32 => Some(115),
        33 => Some(118),
        34 => Some(119),
        35 => Some(120),
        36 => Some(121),
        37 => Some(122),
        38 => Some(123),
        39 => Some(124),
        40 => Some(125),
        41 => Some(126),
        43 => Some(131),
        44 => Some(82),
        45 => Some(136),
        46 => Some(143),
        47 => Some(144),
        48 => Some(145),
        49 => Some(149),
        50 => Some(82),
        51 => Some(132),
        52 => Some(82),
        53 => Some(133),
        54 => Some(82),
        55 => Some(135),
        56 => Some(139),
        57 => Some(150),
        58 => Some(153),
        59 => Some(154),
        60 => Some(155),
        61 => Some(156),
        62 => Some(157),
        63 => Some(82),
        64 => Some(158),
        65 => Some(159),
        66 => Some(160),
        67 => Some(171),
        68 => Some(82),
        69 => Some(82),
        70 => Some(161),
        71 => Some(162),
        72 => Some(163),
        73 => Some(82),
        74 => Some(82),
        75 => Some(165),
        76 => Some(164),
        77 => Some(82),
        78 => Some(82),
        79 => Some(166),
        80 => Some(82),
        81 => Some(82),
        82 => Some(167),
        83 => Some(168),
        84 => Some(168),
        85 => Some(133),
        86 => Some(82),
        87 => Some(82),
        88 => Some(169),
        89 => Some(170),
        90 => Some(172),
        91 => Some(173),
        92 => Some(175),
        93 => Some(132),
        94 => Some(176),
        95 => Some(177),
        96 => Some(82),
        97 => Some(178),
        98 => Some(179),
        99 => Some(180),
        100 => Some(181),
        101 => Some(182),
        102 => Some(183),
        103 => Some(185),
        104 => Some(186),
        105 => Some(187),
        106 => Some(140),
        107 => Some(141),
        108 => Some(142),
        109 => Some(188),
        110 => Some(189),
        111 => Some(82),
        112 => Some(190),
        116 => Some(191),
        117 => Some(191),
        118 => Some(191),
        119 => Some(191),
        _ => None,
    }
}

/// Decodes the full `replay.game.events` stream into a list of
/// [`GameEvent`]s, in chronological order.
///
/// Each event is prefixed by a gameloop delta ([`read_var_uint32`]) and a
/// user id (`replay_userid_typeid`, typeid 8 -> typeid 2: `_int(0,5)`),
/// then a 7-bit event id (`game_eventid_typeid`, typeid 0). Event ids 27
/// (`SCmdEvent`), 28 (`SSelectionDeltaEvent`), and 29
/// (`SControlGroupUpdateEvent`) are decoded fully; every other id present
/// in `game_event_types` is skipped via [`skip_bitpacked_value`] without
/// being modeled; an id absent from `game_event_types` entirely returns
/// [`GameEventsError::UnsupportedEventId`] (see the module doc). After
/// each event, the stream re-aligns to the next byte boundary.
///
/// # Errors
/// Returns [`GameEventsError::UnsupportedEventId`] if an event id isn't
/// present in `game_event_types` at all, or
/// [`GameEventsError::UnknownChoiceVariant`] if skipping an unmodeled
/// event hits a `_choice` selector with no matching variant.
pub fn decode_game_events(bytes: &[u8]) -> Result<Vec<GameEvent>, GameEventsError> {
    let mut bit_pos = 0usize;
    let mut gameloop: i64 = 0;
    let total_bits = bytes.len() * 8;
    // Pre-size on a rough bytes-per-event estimate (same rationale as
    // decode_tracker_events' pre-sizing): only Cmd/SelectionDelta/
    // ControlGroupUpdate events actually materialize, so this slightly
    // over-reserves; a wrong guess costs a little memory, never
    // correctness.
    let mut events = Vec::with_capacity(bytes.len() / 48);

    while bit_pos < total_bits {
        let delta = read_var_uint32(bytes, &mut bit_pos);
        gameloop += delta;

        let user_id = read_int(bytes, &mut bit_pos, 0, 5);
        let event_id = read_int(bytes, &mut bit_pos, 0, 7);

        match typeid_for_event(event_id) {
            Some(100) => {
                events.push(GameEvent::Cmd(decode_cmd_event(
                    bytes,
                    &mut bit_pos,
                    gameloop,
                    user_id,
                )));
            }
            Some(109) => {
                events.push(GameEvent::SelectionDelta(decode_selection_delta_event(
                    bytes,
                    &mut bit_pos,
                    gameloop,
                    user_id,
                )));
            }
            Some(110) => {
                events.push(GameEvent::ControlGroupUpdate(decode_control_group_update_event(
                    bytes,
                    &mut bit_pos,
                    gameloop,
                    user_id,
                )));
            }
            Some(other_typeid) => {
                skip_bitpacked_value(bytes, &mut bit_pos, other_typeid)?;
            }
            None => {
                return Err(GameEventsError::UnsupportedEventId { event_id, bit_pos });
            }
        }

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

    /// Writes `n` bits of `value` starting at `*bit_pos`, using the exact
    /// inverse of [`crate::bitpacked::read_bits`]'s chunking algorithm
    /// (byte-boundary-aligned chunks, earlier chunks more significant) —
    /// lets tests build realistic multi-field bitstreams by value instead
    /// of hand-computing raw bytes.
    fn write_bits(bytes: &mut Vec<u8>, bit_pos: &mut usize, value: u64, n: u32) {
        let mut remaining = n;
        while remaining > 0 {
            let byte_idx = *bit_pos / 8;
            let bit_offset = (*bit_pos % 8) as u32;
            while bytes.len() <= byte_idx {
                bytes.push(0);
            }
            let space_in_byte = 8 - bit_offset;
            let copy_bits = remaining.min(space_in_byte);
            let chunk = (value >> (remaining - copy_bits)) & ((1u64 << copy_bits) - 1);
            bytes[byte_idx] |= (chunk as u8) << bit_offset;
            *bit_pos += copy_bits as usize;
            remaining -= copy_bits;
        }
    }

    #[test]
    fn write_bits_round_trips_through_read_bits() {
        // Sanity-checks the test helper itself against the already-proven
        // reader before trusting it in the events tests below.
        let mut bytes = Vec::new();
        let mut write_pos = 0;
        write_bits(&mut bytes, &mut write_pos, 18, 12);
        assert_eq!(bytes, vec![0x01, 0x02]);

        let mut read_pos = 0;
        assert_eq!(read_bits(&bytes, &mut read_pos, 12), 18);
    }

    #[test]
    fn decodes_minimal_all_zero_selection_delta_event() {
        // control_group_id(4) + subgroup_index(9) + remove_mask selector
        // (2, ->None) + add_subgroups count(9, ->0 elements) +
        // add_unit_tags count(9, ->0 elements) = 33 bits.
        let bytes = [0u8; 5];
        let mut bit_pos = 0;

        let event = decode_selection_delta_event(&bytes, &mut bit_pos, 42, 3);

        assert_eq!(event.gameloop, 42);
        assert_eq!(event.user_id, 3);
        assert_eq!(event.control_group_id, 0);
        assert_eq!(event.subgroup_index, 0);
        assert!(matches!(event.remove_mask, SelectionMask::None));
        assert!(event.add_subgroups.is_empty());
        assert!(event.add_unit_tags.is_empty());
        assert_eq!(bit_pos, 33);
    }

    #[test]
    fn selection_delta_event_exposes_newly_added_unit_tags() {
        // Builds a realistic delta: control_group_id=0, subgroup_index=0,
        // remove_mask=None, no add_subgroups, one add_unit_tags entry
        // encoding a real TargetUnit-style tag (unit_tag_index=5,
        // unit_tag_recycle=1 -> tag = (5<<18)|1).
        let tag = (5i64 << 18) | 1;
        let mut bytes = Vec::new();
        let mut pos = 0;
        write_bits(&mut bytes, &mut pos, 0, 4); // control_group_id
        write_bits(&mut bytes, &mut pos, 0, 9); // subgroup_index
        write_bits(&mut bytes, &mut pos, 0, 2); // remove_mask selector -> None
        write_bits(&mut bytes, &mut pos, 0, 9); // add_subgroups count
        write_bits(&mut bytes, &mut pos, 1, 9); // add_unit_tags count
        write_bits(&mut bytes, &mut pos, tag as u64, 32);

        let mut bit_pos = 0;
        let event = decode_selection_delta_event(&bytes, &mut bit_pos, 100, 0);

        assert_eq!(event.add_unit_tags, vec![tag]);
        assert_eq!(bit_pos, pos);
    }

    #[test]
    fn selection_delta_event_decodes_a_one_indices_remove_mask() {
        let mut bytes = Vec::new();
        let mut pos = 0;
        write_bits(&mut bytes, &mut pos, 0, 4); // control_group_id
        write_bits(&mut bytes, &mut pos, 0, 9); // subgroup_index
        write_bits(&mut bytes, &mut pos, 2, 2); // remove_mask selector -> OneIndices
        write_bits(&mut bytes, &mut pos, 2, 9); // OneIndices count = 2
        write_bits(&mut bytes, &mut pos, 0, 9); // index 0
        write_bits(&mut bytes, &mut pos, 3, 9); // index 3
        write_bits(&mut bytes, &mut pos, 0, 9); // add_subgroups count
        write_bits(&mut bytes, &mut pos, 0, 9); // add_unit_tags count

        let mut bit_pos = 0;
        let event = decode_selection_delta_event(&bytes, &mut bit_pos, 0, 0);

        match event.remove_mask {
            SelectionMask::OneIndices(indices) => assert_eq!(indices, vec![0, 3]),
            other => panic!("expected OneIndices, got {other:?}"),
        }
    }

    #[test]
    fn decodes_minimal_all_zero_control_group_update_event() {
        // control_group_index(4) + control_group_update(3) + mask
        // selector(2, ->None) = 9 bits.
        let bytes = [0u8; 2];
        let mut bit_pos = 0;

        let event = decode_control_group_update_event(&bytes, &mut bit_pos, 7, 1);

        assert_eq!(event.gameloop, 7);
        assert_eq!(event.user_id, 1);
        assert_eq!(event.control_group_index, 0);
        assert_eq!(event.control_group_update, 0);
        assert!(matches!(event.mask, SelectionMask::None));
        assert_eq!(bit_pos, 9);
    }

    #[test]
    fn control_group_update_event_decodes_the_recall_update_type() {
        let mut bytes = Vec::new();
        let mut pos = 0;
        write_bits(&mut bytes, &mut pos, 5, 4); // control_group_index
        write_bits(&mut bytes, &mut pos, 2, 3); // control_group_update = 2 (Get/Recall)
        write_bits(&mut bytes, &mut pos, 0, 2); // mask selector -> None

        let mut bit_pos = 0;
        let event = decode_control_group_update_event(&bytes, &mut bit_pos, 0, 0);

        assert_eq!(event.control_group_index, 5);
        assert_eq!(event.control_group_update, 2);
    }

    // skip_bitpacked_value tests below use real typeids from
    // protocol97425's typeinfos table (via crate::typeinfos::typeinfo),
    // chosen to be small, representative examples of each TypeInfo kind
    // reachable from game_event_types — not synthetic data, so these
    // double as a sanity check that the generated table's structure is
    // wired correctly, on top of exercising skip_bitpacked_value itself.

    #[test]
    fn skips_int() {
        // typeid 0: _int(0, 7).
        let bytes = [0xFFu8];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 0).unwrap();
        assert_eq!(bit_pos, 7);
    }

    #[test]
    fn skips_bool() {
        // typeid 13: _bool (a 1-bit int in bit-packed mode).
        let bytes = [0x00u8];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 13).unwrap();
        assert_eq!(bit_pos, 1);
    }

    #[test]
    fn skips_optional_absent() {
        // typeid 25: _optional(10) where 10 is _int(0,8). Presence bit 0
        // -> only the presence bit itself is consumed.
        let bytes = [0x00u8];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 25).unwrap();
        assert_eq!(bit_pos, 1);
    }

    #[test]
    fn skips_optional_present() {
        // typeid 25 again, presence bit 1 this time -> presence bit + 8
        // inner bits.
        let bytes = [0xFFu8, 0xFFu8];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 25).unwrap();
        assert_eq!(bit_pos, 9);
    }

    #[test]
    fn skips_struct() {
        // typeid 8: _struct { m_userId: <typeid 2> } where typeid 2 is
        // _int(0,5) -> only the single field's 5 bits are consumed.
        let bytes = [0xFFu8];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 8).unwrap();
        assert_eq!(bit_pos, 5);
    }

    #[test]
    fn skips_choice_all_variants() {
        // typeid 7 (SVarUint32): bound (0,2), variants 0->6 bits,
        // 1->14 bits, 2->22 bits, 3->32 bits. The 2-bit selector is read
        // from the low bits of the stream, so the selector value is
        // controlled via the low 2 bits of the first byte.
        let mut bit_pos = 0;
        skip_bitpacked_value(&[0b0000_0000], &mut bit_pos, 7).unwrap();
        assert_eq!(bit_pos, 2 + 6, "selector 0 -> 6-bit variant");

        let mut bit_pos = 0;
        skip_bitpacked_value(&[0b0000_0001, 0x00], &mut bit_pos, 7).unwrap();
        assert_eq!(bit_pos, 2 + 14, "selector 1 -> 14-bit variant");

        let mut bit_pos = 0;
        skip_bitpacked_value(&[0b0000_0010, 0x00, 0x00], &mut bit_pos, 7).unwrap();
        assert_eq!(bit_pos, 2 + 22, "selector 2 -> 22-bit variant");

        let mut bit_pos = 0;
        skip_bitpacked_value(&[0b0000_0011, 0x00, 0x00, 0x00, 0x00], &mut bit_pos, 7).unwrap();
        assert_eq!(bit_pos, 2 + 32, "selector 3 -> 32-bit variant");
    }

    #[test]
    fn skips_unknown_choice_variant_with_a_typed_error() {
        // typeid 134: bound (0,3) (8 possible selector values) but only
        // variants 0-5 are defined -> selector 6 has no matching variant.
        let bytes = [0b0000_0110u8];
        let mut bit_pos = 0;
        let result = skip_bitpacked_value(&bytes, &mut bit_pos, 134);
        assert!(matches!(
            result,
            Err(GameEventsError::UnknownChoiceVariant { typeid: 134, selector: 6, .. })
        ));
    }

    #[test]
    fn skips_array() {
        // typeid 66: _array(bound=(0,3), element=<typeid 6>), typeid 6 is
        // _int(0,32). Count 2, encoded in the low 3 bits of the first
        // byte -> 3 (count field) + 2*32 (elements) = 67 bits.
        let bytes = [0b0000_0010u8, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 66).unwrap();
        assert_eq!(bit_pos, 3 + 2 * 32);
    }

    #[test]
    fn skips_blob() {
        // typeid 9: _blob(bound=(0,8)) -> an 8-bit length, byte-align
        // (already aligned here), then that many bytes. Length 3.
        let bytes = [3u8, 0, 0, 0];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 9).unwrap();
        assert_eq!(bit_pos, 8 + 3 * 8);
    }

    #[test]
    fn skips_fourcc() {
        // typeid 19: _fourcc -> a fixed 32 bits.
        let bytes = [0u8; 4];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 19).unwrap();
        assert_eq!(bit_pos, 32);
    }

    #[test]
    fn skips_null() {
        // typeid 94: _null -> no bits consumed.
        let bytes = [0xFFu8];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 94).unwrap();
        assert_eq!(bit_pos, 0);
    }

    #[test]
    fn skips_bitarray() {
        // typeid 54: _bitarray(bound=(0,6)) -> a 6-bit length, then that
        // many further (unaligned) bits. Length 3, in the low 6 bits of
        // the first byte.
        let bytes = [0b0000_0011u8, 0x00];
        let mut bit_pos = 0;
        skip_bitpacked_value(&bytes, &mut bit_pos, 54).unwrap();
        assert_eq!(bit_pos, 6 + 3);
    }
}
