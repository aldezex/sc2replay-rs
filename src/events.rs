//! Decoding of `replay.tracker.events`: the stream of gameplay-tracking
//! events recorded throughout a StarCraft II match (unit creation/death,
//! resource transfers, periodic player stats, etc.).
//!
//! Unlike `replay.details` (a single struct), this file is a *stream* of
//! events, each prefixed by a gameloop delta and an event id that
//! determines which struct follows. See [`decode_tracker_events`] for the
//! entry point.
//!
//! Field layout is cross-checked against a build close to
//! `protocol97425` — see the crate-level notes on protocol versioning.

use crate::protocol::{
    read_array, read_blob, read_choice_as_int, read_optional, read_struct, read_tagged_int,
    skip_value,
};

/// A single decoded tracker event, tagged with the gameloop (game tick)
/// at which it occurred.
///
/// Only the 10 known `NNet.Replay.Tracker.*Event` types are modeled
/// explicitly; anything else falls back to [`TrackerEvent::Unknown`].
#[derive(Debug)]
pub enum TrackerEvent {
    /// Periodic per-player economy/army snapshot (`eventid` 0).
    PlayerStats {
        gameloop: i64,
        player_id: i64,
        stats: PlayerStats,
    },
    /// A unit came into existence, either built or already present at
    /// game start (`eventid` 1).
    UnitBorn {
        gameloop: i64,
        unit_tag_index: i64,
        unit_tag_recycle: i64,
        unit_type_name: String,
        control_player_id: i64,
        upkeep_player_id: i64,
        x: i64,
        y: i64,
        creator_unit_tag_index: Option<i64>,
        creator_unit_tag_recycle: Option<i64>,
        creator_ability_name: Option<String>,
    },
    /// A unit died (`eventid` 2). May follow either `UnitInit` or
    /// `UnitBorn` for the same unit tag.
    UnitDied {
        gameloop: i64,
        unit_tag_index: i64,
        unit_tag_recycle: i64,
        killer_player_id: Option<i64>,
        x: i64,
        y: i64,
        killer_unit_tag_index: Option<i64>,
        killer_unit_tag_recycle: Option<i64>,
    },
    /// A unit changed controlling/upkeep player, e.g. via mind control
    /// or a hero swap (`eventid` 3).
    UnitOwnerChange {
        gameloop: i64,
        unit_tag_index: i64,
        unit_tag_recycle: i64,
        control_player_id: i64,
        upkeep_player_id: i64,
    },
    /// A unit morphed into a different unit type (`eventid` 4).
    UnitTypeChange {
        gameloop: i64,
        unit_tag_index: i64,
        unit_tag_recycle: i64,
        unit_type_name: String,
    },
    /// A player researched or gained an upgrade (`eventid` 5).
    Upgrade {
        gameloop: i64,
        player_id: i64,
        upgrade_type_name: String,
        count: i64,
    },
    /// A structure/unit started being built (placed under construction)
    /// (`eventid` 6). Followed later by `UnitDone` when construction
    /// finishes.
    UnitInit {
        gameloop: i64,
        unit_tag_index: i64,
        unit_tag_recycle: i64,
        unit_type_name: String,
        control_player_id: i64,
        upkeep_player_id: i64,
        x: i64,
        y: i64,
    },
    /// A unit finished construction (`eventid` 7).
    UnitDone {
        gameloop: i64,
        unit_tag_index: i64,
        unit_tag_recycle: i64,
    },
    /// Periodic batch of approximate positions for units that have
    /// dealt or taken damage (`eventid` 8), capped at 256 units per
    /// event. `items` is a flat list of `(delta_index, x, y)` triples —
    /// see the module docs of the reference `s2protocol` implementation
    /// for the exact reconstruction formula (positions are in units of
    /// 1/4, and unit indices accumulate via `delta_index` starting from
    /// `first_unit_index`).
    UnitPositions {
        gameloop: i64,
        first_unit_index: i64,
        items: Vec<i64>,
    },
    /// A player's slot setup at game start (`eventid` 9).
    PlayerSetup {
        gameloop: i64,
        player_id: i64,
        setup_type: i64,
        user_id: Option<i64>,
        slot_id: Option<i64>,
    },
    /// An event id not in the known set of 10 tracker event types.
    Unknown { gameloop: i64, event_id: i64 },
}

/// Economy and army snapshot carried by [`TrackerEvent::PlayerStats`].
///
/// All fields are raw integers as stored in the replay, with one
/// exception: `food_used` and `food_made` are fixed-point — divide by
/// 4096 to get the real (fractional) supply value.
#[derive(Debug, Default)]
pub struct PlayerStats {
    pub minerals_current: i64,
    pub vespene_current: i64,
    pub minerals_collection_rate: i64,
    pub vespene_collection_rate: i64,
    pub workers_active_count: i64,
    pub minerals_used_in_progress_army: i64,
    pub minerals_used_in_progress_economy: i64,
    pub minerals_used_in_progress_technology: i64,
    pub vespene_used_in_progress_army: i64,
    pub vespene_used_in_progress_economy: i64,
    pub vespene_used_in_progress_technology: i64,
    pub minerals_used_current_army: i64,
    pub minerals_used_current_economy: i64,
    pub minerals_used_current_technology: i64,
    pub vespene_used_current_army: i64,
    pub vespene_used_current_economy: i64,
    pub vespene_used_current_technology: i64,
    pub minerals_lost_army: i64,
    pub minerals_lost_economy: i64,
    pub minerals_lost_technology: i64,
    pub vespene_lost_army: i64,
    pub vespene_lost_economy: i64,
    pub vespene_lost_technology: i64,
    pub minerals_killed_army: i64,
    pub minerals_killed_economy: i64,
    pub minerals_killed_technology: i64,
    pub vespene_killed_army: i64,
    pub vespene_killed_economy: i64,
    pub vespene_killed_technology: i64,
    /// Fixed-point: divide by 4096 for the real value.
    pub food_used: i64,
    /// Fixed-point: divide by 4096 for the real value.
    pub food_made: i64,
    pub minerals_used_active_forces: i64,
    pub vespene_used_active_forces: i64,
    pub minerals_friendly_fire_army: i64,
    pub minerals_friendly_fire_economy: i64,
    pub minerals_friendly_fire_technology: i64,
    pub vespene_friendly_fire_army: i64,
    pub vespene_friendly_fire_economy: i64,
    pub vespene_friendly_fire_technology: i64,
}

/// Decodes an `SUnitDoneEvent` (typeid 205).
fn decode_unit_done(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut unit_tag_index = 0;
    let mut unit_tag_recycle = 0;

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => unit_tag_index = read_tagged_int(b, p),
        1 => unit_tag_recycle = read_tagged_int(b, p),
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::UnitDone {
        gameloop,
        unit_tag_index,
        unit_tag_recycle,
    }
}

/// Decodes an `SUnitOwnerChangeEvent` (typeid 201).
fn decode_unit_owner_change(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut unit_tag_index = 0;
    let mut unit_tag_recycle = 0;
    let mut control_player_id = 0;
    let mut upkeep_player_id = 0;

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => unit_tag_index = read_tagged_int(b, p),
        1 => unit_tag_recycle = read_tagged_int(b, p),
        2 => control_player_id = read_tagged_int(b, p),
        3 => upkeep_player_id = read_tagged_int(b, p),
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::UnitOwnerChange {
        gameloop,
        unit_tag_index,
        unit_tag_recycle,
        control_player_id,
        upkeep_player_id,
    }
}

/// Decodes an `SUpgradeEvent` (typeid 203).
fn decode_upgrade(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut player_id = 0;
    let mut upgrade_type_name = String::new();
    let mut count = 0;

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => player_id = read_tagged_int(b, p),
        1 => upgrade_type_name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        2 => count = read_tagged_int(b, p),
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::Upgrade {
        gameloop,
        player_id,
        upgrade_type_name,
        count,
    }
}

/// Decodes an `SUnitTypeChangeEvent` (typeid 202).
fn decode_unit_type_change(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut unit_tag_index = 0;
    let mut unit_tag_recycle = 0;
    let mut unit_type_name = String::new();

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => unit_tag_index = read_tagged_int(b, p),
        1 => unit_tag_recycle = read_tagged_int(b, p),
        2 => unit_type_name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::UnitTypeChange {
        gameloop,
        unit_tag_index,
        unit_tag_recycle,
        unit_type_name,
    }
}

/// Decodes an `SUnitDiedEvent` (typeid 200).
fn decode_unit_died(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut unit_tag_index = 0;
    let mut unit_tag_recycle = 0;
    let mut killer_player_id: Option<i64> = None;
    let mut x = 0;
    let mut y = 0;
    let mut killer_unit_tag_index: Option<i64> = None;
    let mut killer_unit_tag_recycle: Option<i64> = None;

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => unit_tag_index = read_tagged_int(b, p),
        1 => unit_tag_recycle = read_tagged_int(b, p),
        2 => killer_player_id = read_optional(b, p, read_tagged_int),
        3 => x = read_tagged_int(b, p),
        4 => y = read_tagged_int(b, p),
        5 => killer_unit_tag_index = read_optional(b, p, read_tagged_int),
        6 => killer_unit_tag_recycle = read_optional(b, p, read_tagged_int),
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::UnitDied {
        gameloop,
        unit_tag_index,
        unit_tag_recycle,
        killer_player_id,
        x,
        y,
        killer_unit_tag_index,
        killer_unit_tag_recycle,
    }
}

/// Decodes an `SUnitInitEvent` (typeid 204).
fn decode_unit_init(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut unit_tag_index = 0;
    let mut unit_tag_recycle = 0;
    let mut unit_type_name = String::new();
    let mut control_player_id = 0;
    let mut upkeep_player_id = 0;
    let mut x = 0;
    let mut y = 0;

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => unit_tag_index = read_tagged_int(b, p),
        1 => unit_tag_recycle = read_tagged_int(b, p),
        2 => unit_type_name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        3 => control_player_id = read_tagged_int(b, p),
        4 => upkeep_player_id = read_tagged_int(b, p),
        5 => x = read_tagged_int(b, p),
        6 => y = read_tagged_int(b, p),
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::UnitInit {
        gameloop,
        unit_tag_index,
        unit_tag_recycle,
        unit_type_name,
        control_player_id,
        upkeep_player_id,
        x,
        y,
    }
}

/// Decodes an `SUnitBornEvent` (typeid 199).
fn decode_unit_born(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut unit_tag_index = 0;
    let mut unit_tag_recycle = 0;
    let mut unit_type_name = String::new();
    let mut control_player_id = 0;
    let mut upkeep_player_id = 0;
    let mut x = 0;
    let mut y = 0;
    let mut creator_unit_tag_index: Option<i64> = None;
    let mut creator_unit_tag_recycle: Option<i64> = None;
    let mut creator_ability_name: Option<String> = None;

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => unit_tag_index = read_tagged_int(b, p),
        1 => unit_tag_recycle = read_tagged_int(b, p),
        2 => unit_type_name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        3 => control_player_id = read_tagged_int(b, p),
        4 => upkeep_player_id = read_tagged_int(b, p),
        5 => x = read_tagged_int(b, p),
        6 => y = read_tagged_int(b, p),
        7 => creator_unit_tag_index = read_optional(b, p, read_tagged_int),
        8 => creator_unit_tag_recycle = read_optional(b, p, read_tagged_int),
        9 => {
            creator_ability_name = read_optional(b, p, |b2, p2| {
                String::from_utf8_lossy(read_blob(b2, p2)).to_string()
            })
        }
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::UnitBorn {
        gameloop,
        unit_tag_index,
        unit_tag_recycle,
        unit_type_name,
        control_player_id,
        upkeep_player_id,
        x,
        y,
        creator_unit_tag_index,
        creator_unit_tag_recycle,
        creator_ability_name,
    }
}

/// Decodes an `SPlayerSetupEvent` (typeid 208).
fn decode_player_setup(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut player_id = 0;
    let mut setup_type = 0;
    let mut user_id: Option<i64> = None;
    let mut slot_id: Option<i64> = None;

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => player_id = read_tagged_int(b, p),
        1 => setup_type = read_tagged_int(b, p),
        2 => user_id = read_optional(b, p, read_tagged_int),
        3 => slot_id = read_optional(b, p, read_tagged_int),
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::PlayerSetup {
        gameloop,
        player_id,
        setup_type,
        user_id,
        slot_id,
    }
}

/// Decodes an `SUnitPositionsEvent` (typeid 207).
fn decode_unit_positions(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut first_unit_index = 0;
    let mut items = Vec::new();

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => first_unit_index = read_tagged_int(b, p),
        1 => items = read_array(b, p, read_tagged_int),
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::UnitPositions {
        gameloop,
        first_unit_index,
        items,
    }
}

/// Decodes an `SPlayerStatsEvent` (typeid 197), including its nested
/// 39-field `m_stats` struct (typeid 196) via the [`read_int_fields`]
/// macro.
fn decode_player_stats(bytes: &[u8], pos: &mut usize, gameloop: i64) -> TrackerEvent {
    let mut player_id = 0;
    let mut stats = PlayerStats::default();

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => player_id = read_tagged_int(b, p),
        1 => {
            read_struct(b, p, |b2, p2, inner_field_index| {
                read_int_fields!(b2, p2, inner_field_index, {
                    0 => stats.minerals_current,
                    1 => stats.vespene_current,
                    2 => stats.minerals_collection_rate,
                    3 => stats.vespene_collection_rate,
                    4 => stats.workers_active_count,
                    5 => stats.minerals_used_in_progress_army,
                    6 => stats.minerals_used_in_progress_economy,
                    7 => stats.minerals_used_in_progress_technology,
                    8 => stats.vespene_used_in_progress_army,
                    9 => stats.vespene_used_in_progress_economy,
                    10 => stats.vespene_used_in_progress_technology,
                    11 => stats.minerals_used_current_army,
                    12 => stats.minerals_used_current_economy,
                    13 => stats.minerals_used_current_technology,
                    14 => stats.vespene_used_current_army,
                    15 => stats.vespene_used_current_economy,
                    16 => stats.vespene_used_current_technology,
                    17 => stats.minerals_lost_army,
                    18 => stats.minerals_lost_economy,
                    19 => stats.minerals_lost_technology,
                    20 => stats.vespene_lost_army,
                    21 => stats.vespene_lost_economy,
                    22 => stats.vespene_lost_technology,
                    23 => stats.minerals_killed_army,
                    24 => stats.minerals_killed_economy,
                    25 => stats.minerals_killed_technology,
                    26 => stats.vespene_killed_army,
                    27 => stats.vespene_killed_economy,
                    28 => stats.vespene_killed_technology,
                    29 => stats.food_used,
                    30 => stats.food_made,
                    31 => stats.minerals_used_active_forces,
                    32 => stats.vespene_used_active_forces,
                    33 => stats.minerals_friendly_fire_army,
                    34 => stats.minerals_friendly_fire_economy,
                    35 => stats.minerals_friendly_fire_technology,
                    36 => stats.vespene_friendly_fire_army,
                    37 => stats.vespene_friendly_fire_economy,
                    38 => stats.vespene_friendly_fire_technology,
                });
            });
        }
        _ => skip_value(b, p).unwrap(),
    });

    TrackerEvent::PlayerStats {
        gameloop,
        player_id,
        stats,
    }
}

/// Decodes the full `replay.tracker.events` stream into a list of
/// [`TrackerEvent`]s, in chronological order.
///
/// Each event in the stream is prefixed by a gameloop delta (encoded as
/// an `SVarUint32` `choice`, see [`crate::protocol::read_choice_as_int`])
/// and an event id selecting which of the 10 known struct layouts
/// follows. Event ids outside that set decode to [`TrackerEvent::Unknown`]
/// rather than erroring, since the byte layout of an unrecognized event
/// can't be skipped without knowing its type.
pub fn decode_tracker_events(bytes: &[u8]) -> Vec<TrackerEvent> {
    let mut pos = 0;
    let mut gameloop = 0;
    // Rough events-per-byte estimate (real streams measure ~35-60
    // bytes/event) to pre-size the vector — an upper-ish guess only
    // trades a little memory for skipping most grow-and-copy cycles on
    // multi-thousand-event streams; correctness is unaffected either way.
    let mut events = Vec::with_capacity(bytes.len() / 32);

    while pos < bytes.len() {
        let delta = read_choice_as_int(bytes, &mut pos);
        gameloop += delta;

        let event_id = read_tagged_int(bytes, &mut pos);

        let event = match event_id {
            0 => decode_player_stats(bytes, &mut pos, gameloop),
            1 => decode_unit_born(bytes, &mut pos, gameloop),
            2 => decode_unit_died(bytes, &mut pos, gameloop),
            3 => decode_unit_owner_change(bytes, &mut pos, gameloop),
            4 => decode_unit_type_change(bytes, &mut pos, gameloop),
            5 => decode_upgrade(bytes, &mut pos, gameloop),
            6 => decode_unit_init(bytes, &mut pos, gameloop),
            7 => decode_unit_done(bytes, &mut pos, gameloop),
            8 => decode_unit_positions(bytes, &mut pos, gameloop),
            9 => decode_player_setup(bytes, &mut pos, gameloop),
            other => TrackerEvent::Unknown {
                gameloop,
                event_id: other,
            },
        };

        events.push(event);
    }

    events
}
