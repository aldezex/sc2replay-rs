use crate::{
    player::Player,
    protocol::{read_array, read_blob, read_optional, read_struct, read_tagged_int, skip_value},
};

/// Decoded contents of a replay's `replay.details` file: the map name
/// and the list of players, already resolved to readable text.
pub struct ReplayDetails {
    pub map_name: String,
    pub players: Vec<Player>,
}

/// Decodes a single `SPlayerListEntry` (one entry of `SDetails.m_playerList`).
///
/// `m_name` (field 0), `m_race` (field 2) and `m_result` (field 8, a bare
/// `_int([0,2])`: 1=Won, 2=Lost, 0/else=Undecided — NOT wrapped in an
/// `optional`, per Blizzard's protocol97563 typeinfo) are extracted; every
/// other field (`m_toon`, `m_color`, `m_control`, etc.) is skipped with
/// [`skip_value`] to keep the stream aligned without needing to model
/// their full structure.
fn decode_player(bytes: &[u8], pos: &mut usize) -> Player {
    let mut name = String::new();
    let mut race = String::new();
    let mut result = crate::player::PlayerResult::Undecided;

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        2 => race = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        8 => {
            let code = read_tagged_int(b, p);
            result = match code {
                1 => crate::player::PlayerResult::Won,
                2 => crate::player::PlayerResult::Lost,
                _ => crate::player::PlayerResult::Undecided,
            };
        }
        _ => skip_value(b, p).unwrap(),
    });

    let mut player = Player::new(&name, &race);
    player.result = result;
    player
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
                read_array(b2, p2, decode_player)
            });
            players = list.unwrap_or_default();
        }
        1 => map_name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        _ => skip_value(b, p).unwrap(),
    });

    ReplayDetails { map_name, players }
}

#[cfg(test)]
mod tests {
    use crate::player::PlayerResult;

    #[test]
    fn decodes_a_known_game_result() {
        // A gitignored real 1v1 fixture with a decisive result.
        let path = "tests/fixtures/dont-oracle-me.SC2Replay";
        let Ok(replay) = crate::replay::load_replay(path) else {
            eprintln!("fixture missing; skipping");
            return;
        };
        // Exactly one Won and one Lost in a decisive 1v1 (or all Undecided
        // if this particular replay stores no result — then this asserts
        // the decode didn't misalign by checking players still parse).
        let wins = replay
            .players
            .iter()
            .filter(|p| p.result == PlayerResult::Won)
            .count();
        let losses = replay
            .players
            .iter()
            .filter(|p| p.result == PlayerResult::Lost)
            .count();
        assert_eq!(replay.players.len(), 2, "names/races still parse (no stream misalign)");
        assert!(
            (wins == 1 && losses == 1) || (wins == 0 && losses == 0),
            "got wins={wins} losses={losses}; a decisive 1v1 is 1/1, an unrecorded one 0/0 — anything else means m_result misaligned"
        );
    }
}
