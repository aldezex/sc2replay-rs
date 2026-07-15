use crate::{
    player::Player,
    protocol::{read_array, read_blob, read_optional, read_struct, skip_value},
};

/// Decoded contents of a replay's `replay.details` file: the map name
/// and the list of players, already resolved to readable text.
pub struct ReplayDetails {
    pub map_name: String,
    pub players: Vec<Player>,
}

/// Decodes a single `SPlayerListEntry` (one entry of `SDetails.m_playerList`).
///
/// Only `m_name` (field 0) and `m_race` (field 2) are extracted; every
/// other field (`m_toon`, `m_color`, `m_control`, etc.) is skipped with
/// [`skip_value`] to keep the stream aligned without needing to model
/// their full structure.
fn decode_player(bytes: &[u8], pos: &mut usize) -> Player {
    let mut name = String::new();
    let mut race = String::new();

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        2 => race = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        _ => skip_value(b, p).unwrap(),
    });

    Player::new(&name, &race)
}

/// Decodes the top-level `SDetails` struct from the raw, already
/// extracted (and decompressed, if applicable) contents of
/// `replay.details`.
///
/// Only two of `SDetails`' ~18 fields are decoded: `m_playerList`
/// (field 0) and `m_title`/map name (field 1). Everything else is
/// skipped. Note `m_playerList` is an `optional<array<...>>`, not a bare
/// array — decoding it directly as an array will misalign the stream.
pub fn decode_replay_details(bytes: &[u8]) -> ReplayDetails {
    let mut pos = 0;
    let mut map_name = String::new();
    let mut players = Vec::new();

    read_struct(bytes, &mut pos, |b, p, field_index| match field_index {
        0 => {
            let list = read_optional(b, p, |b2, p2| {
                read_array(b2, p2, |b3, p3| decode_player(b3, p3))
            });
            players = list.unwrap_or_default();
        }
        1 => map_name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        _ => skip_value(b, p).unwrap(),
    });

    ReplayDetails { map_name, players }
}
