//! Type-parameter registration helpers for Graphite generics.

pub fn is_upper_camel(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {
            chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        }
        _ => false,
    }
}
