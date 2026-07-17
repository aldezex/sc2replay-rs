//! A single player as decoded from `replay.details`.

use crate::format::format_display_name;

/// The game result for a player, from `SPlayerListEntry.m_result`.
/// SC2 stores this unreliably (often `Undecided`); callers must treat
/// `Undecided` as "unknown", never guess a winner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerResult {
    Won,
    Lost,
    Undecided,
}

/// A player's display name and race, as they appear in `SDetails`.
///
/// `name` has already been run through [`format_display_name`] to resolve
/// SC2's in-game markup (`<sp/>`, escaped `<`/`>`, etc.) into plain text.
#[derive(Debug, Clone)]
pub struct Player {
    pub name: String,
    pub race: String,
    pub result: PlayerResult,
}

impl Player {
    pub fn new(name: &str, race: &str) -> Self {
        Player {
            name: format_display_name(name),
            race: race.to_string(),
            result: PlayerResult::Undecided,
        }
    }
}
