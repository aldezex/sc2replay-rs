//! Text formatting helpers for strings embedded in SC2 replay data.

use regex::Regex;

/// Resolves SC2's in-game markup in player/clan names into plain text.
///
/// Strips color tags (`<s val="RRGGBB">...</s>`) and replaces `<sp/>`
/// with a literal space and the escaped `&lt;`/`&gt;`/`&amp;` entities
/// with their real characters.
///
/// Not exhaustive — covers the markup observed in practice so far, not
/// every form SC2's text markup can theoretically take.
pub fn format_display_name(raw: &str) -> String {
    let color_tag = Regex::new(r#"</?s(?:\s+val="[0-9a-fA-F]{6,8}")?>"#).unwrap();

    let without_color = color_tag.replace_all(raw, "");

    without_color
        .replace("<sp/>", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
