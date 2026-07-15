use crate::format::format_display_name;

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
