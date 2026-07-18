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
    /// `SPlayerListEntry.m_workingSetSlotId` — this player's index into
    /// `replay.initData`'s 16-entry lobby slot array. Load-bearing rather
    /// than informational: it is the only non-heuristic way to attribute
    /// a lobby slot's MMR to a player, since the slot array also holds
    /// observers and empty slots and so does not line up positionally
    /// with `m_playerList`.
    pub working_set_slot_id: Option<i64>,
    /// This player's matchmaking rating, joined from
    /// `replay.initData`'s `m_scaledRating` via
    /// [`Self::working_set_slot_id`].
    ///
    /// [`None`] means "not knowable from this replay", never "unrated" or
    /// "zero": custom/tournament-lobby games frequently store no rating,
    /// and SC2 also writes a `-36400` sentinel that
    /// [`crate::init_data`] deliberately refuses to surface as a number.
    /// Callers must exclude these players from any aggregate rather than
    /// substituting a default.
    pub mmr: Option<i64>,
}

impl Player {
    pub fn new(name: &str, race: &str) -> Self {
        Player {
            name: format_display_name(name),
            race: race.to_string(),
            result: PlayerResult::Undecided,
            working_set_slot_id: None,
            mmr: None,
        }
    }
}
