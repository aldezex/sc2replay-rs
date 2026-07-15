use regex::Regex;

pub fn format_display_name(raw: &str) -> String {
    let color_tag = Regex::new(r#"</?s(?:\s+val="[0-9a-fA-F]{6,8}")?>"#).unwrap();

    let without_color = color_tag.replace_all(raw, "");

    without_color
        .replace("<sp/>", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
