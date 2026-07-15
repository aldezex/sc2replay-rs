//! A single player as decoded from `replay.details`.

use crate::format::format_display_name;

/// A player's display name and race, as they appear in `SDetails`.
///
/// `name` has already been run through [`format_display_name`] to resolve
/// SC2's in-game markup (`<sp/>`, escaped `<`/`>`, etc.) into plain text.
#[derive(Debug, Clone)]
pub struct Player {
    pub name: String,
    pub race: String,
}

impl Player {
    pub fn new(name: &str, race: &str) -> Self {
        Player {
            name: format_display_name(name),
            race: race.to_string(),
        }
    }
}
