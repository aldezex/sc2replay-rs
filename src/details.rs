use crate::{
    player::Player,
    protocol::{read_array, read_blob, read_optional, read_struct, skip_value},
};

pub struct ReplayDetails {
    pub map_name: String,
    pub players: Vec<Player>,
}

fn decode_player(bytes: &[u8], pos: &mut usize) -> Player {
    let mut name = String::new();
    let mut race = String::new();

    read_struct(bytes, pos, |b, p, field_index| match field_index {
        0 => name = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        2 => race = String::from_utf8_lossy(read_blob(b, p)).to_string(),
        _ => skip_value(b, p).unwrap(),
    });

    Player::new(&name, race)
}

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
