//! Helper macros used to cut down on repetitive decoding boilerplate.

/// Generates a `match` arm set that decodes several integer fields of a
/// struct in one shot, given a list of `field_index => target` pairs.
///
/// Each `target` is assigned the result of [`crate::protocol::read_tagged_int`]
/// for its corresponding field index; any field index not listed falls
/// through to [`crate::protocol::skip_value`], keeping the byte stream
/// aligned without requiring every field to be named.
///
/// `target` accepts any assignable expression (a plain variable or a
/// struct field access like `stats.minerals_current`), not just simple
/// identifiers — this is what lets the macro be used to fill in the
/// fields of a nested struct like `PlayerStats` directly.
///
/// Written for [`crate::events::decode_player_stats`], whose `m_stats`
/// sub-struct has 39 fields that would otherwise require 39 nearly
/// identical `match` arms.
///
/// # Example
/// ```ignore
/// read_struct(bytes, pos, |b, p, field_index| {
///     read_int_fields!(b, p, field_index, {
///         0 => minerals_current,
///         1 => vespene_current,
///     });
/// });
/// ```
macro_rules! read_int_fields {
    ($bytes:ident, $pos:ident, $field_index:ident, { $($idx:literal => $var:expr),+ $(,)? }) => {
        match $field_index {
            $($idx => $var = read_tagged_int($bytes, $pos),)+
            _ => skip_value($bytes, $pos).unwrap(),
        }
    };
}
