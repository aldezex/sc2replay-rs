//! High-level, one-call replay loading — the equivalent of sc2reader's
//! own `sc2reader.load_replay()` in the Python original.
//!
//! Wraps the full pipeline (MPQ container → hash/block tables → file
//! lookup/extraction → protocol decoding) that would otherwise need to
//! be repeated by hand in every consumer of this crate.

use mpq_parser::archive::{extract_file, find_file};
use mpq_parser::block::parse_block_table_entries;
use mpq_parser::crypto::decrypt;
use mpq_parser::crypto::{MPQ_HASH_FILE_KEY, build_crypt_table, hash_string};
use mpq_parser::hash::parse_hash_table_entries;
use mpq_parser::{MpqHeader, MpqParseError, MpqUserDataHeader};

use crate::details::decode_replay_details;
use crate::events::{TrackerEvent, decode_tracker_events};
use crate::game_events::{GameEvent, GameEventsError, decode_game_events};
use crate::header::{ReplayVersion, decode_replay_version};
use crate::player::Player;

/// A fully decoded replay: everything currently extracted from the header
/// (game version/build), `replay.details`, `replay.tracker.events`, and
/// `replay.game.events`.
#[derive(Debug)]
pub struct Replay {
    /// The SC2 game build/version this replay was recorded on — lets a
    /// consumer branch on patch-specific balance constants (e.g. 5.0.16 /
    /// build 97563).
    pub version: ReplayVersion,
    pub map_name: String,
    pub players: Vec<Player>,
    pub tracker_events: Vec<TrackerEvent>,
    pub game_events: Vec<GameEvent>,
}

/// Errors that can occur while loading a replay end-to-end.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("failed to read replay file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse MPQ container: {0}")]
    Mpq(#[from] MpqParseError),
    #[error("required internal file not found in archive: {0}")]
    MissingFile(&'static str),
    /// Unlike `replay.details`/`replay.tracker.events` decoding (both
    /// infallible), `replay.game.events` decoding can fail on an
    /// unsupported event id — see [`GameEventsError`] for why that's
    /// intentional rather than a gap to "fix" by making decoding
    /// infallible again.
    #[error("failed to decode replay.game.events: {0}")]
    GameEvents(#[from] GameEventsError),
}

/// Loads and fully decodes a `.SC2Replay` file from `path`.
///
/// Equivalent to running the MPQ container pipeline (header → hash table
/// → block table → file lookup/extraction) followed by protocol decoding
/// of `replay.details`, `replay.tracker.events`, and `replay.game.events`,
/// in one call.
pub fn load_replay(path: &str) -> Result<Replay, ReplayError> {
    load_replay_from_bytes(&std::fs::read(path)?)
}

/// Loads and fully decodes a `.SC2Replay` already held in memory.
///
/// The whole pipeline operates on byte slices internally, so consumers
/// that receive replay bytes without a backing file — an HTTP upload, an
/// object-storage download — can decode directly instead of round-tripping
/// through a temp file. [`load_replay`] is a thin convenience wrapper over
/// this function.
pub fn load_replay_from_bytes(bytes: &[u8]) -> Result<Replay, ReplayError> {
    let user_header = MpqUserDataHeader::parse(bytes)?;
    let version = decode_replay_version(bytes);
    let offset = user_header.header_offset as usize;
    let mpq_header = MpqHeader::parse(&bytes[offset..])?;

    // The crypt table is a fixed 0x500-entry constant — computed once
    // per process instead of once per replay, which matters for batch
    // workloads loading hundreds of replays (and is free to share
    // across threads).
    static CRYPT_TABLE: std::sync::OnceLock<[u32; mpq_parser::crypto::CRYPT_TABLE_SIZE]> =
        std::sync::OnceLock::new();
    let crypt_table = *CRYPT_TABLE.get_or_init(build_crypt_table);

    let ht_start = offset + mpq_header.hash_table_position as usize;
    let ht_size = mpq_header.hash_table_size as usize * 16;
    let ht_key = hash_string("(hash table)", MPQ_HASH_FILE_KEY, &crypt_table);
    let ht_decrypted = decrypt(&bytes[ht_start..ht_start + ht_size], ht_key, &crypt_table);
    let hash_entries = parse_hash_table_entries(&ht_decrypted);

    let bt_start = offset + mpq_header.block_table_position as usize;
    let bt_size = mpq_header.block_table_size as usize * 16;
    let bt_key = hash_string("(block table)", MPQ_HASH_FILE_KEY, &crypt_table);
    let bt_decrypted = decrypt(&bytes[bt_start..bt_start + bt_size], bt_key, &crypt_table);
    let block_entries = parse_block_table_entries(&bt_decrypted);

    let details_block = find_file(
        "replay.details",
        &hash_entries,
        &block_entries,
        &crypt_table,
    )
    .ok_or(ReplayError::MissingFile("replay.details"))?;
    let details_bytes = extract_file(bytes, offset as u32, *details_block)?;
    let details = decode_replay_details(&details_bytes);

    let tracker_block = find_file(
        "replay.tracker.events",
        &hash_entries,
        &block_entries,
        &crypt_table,
    )
    .ok_or(ReplayError::MissingFile("replay.tracker.events"))?;
    let tracker_bytes = extract_file(bytes, offset as u32, *tracker_block)?;
    let tracker_events = decode_tracker_events(&tracker_bytes);

    let game_events_block = find_file(
        "replay.game.events",
        &hash_entries,
        &block_entries,
        &crypt_table,
    )
    .ok_or(ReplayError::MissingFile("replay.game.events"))?;
    let game_events_bytes = extract_file(bytes, offset as u32, *game_events_block)?;
    let game_events = decode_game_events(&game_events_bytes)?;

    Ok(Replay {
        version,
        map_name: details.map_name,
        players: details.players,
        tracker_events,
        game_events,
    })
}
