//! Shipped layouts. The TOML files under `layouts/` are the single
//! source of truth; they are embedded here so a preset name works with
//! no files on disk, and the same files are what a user copies to edit.

use super::Layout;

pub const TWO_64X32: &str = include_str!("../../layouts/two_64x32.toml");
pub const SIX_PANEL: &str = include_str!("../../layouts/six_panel.toml");

pub const NAMES: [&str; 2] = ["two_64x32", "six_panel"];

/// The preset's TOML text, by name.
pub fn source(name: &str) -> Option<&'static str> {
    match name {
        "two_64x32" => Some(TWO_64X32),
        "six_panel" => Some(SIX_PANEL),
        _ => None,
    }
}

/// Parse a preset by name. The embedded files are checked by the test
/// suite, so a parse failure here is a build defect, not user input.
pub fn load(name: &str) -> Option<Layout> {
    source(name).map(|text| Layout::from_toml(text).expect("embedded preset parses"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_preset_parses_and_carries_its_own_name() {
        for name in NAMES {
            let layout = load(name).expect("preset exists");
            assert_eq!(
                layout.name, name,
                "preset file name and layout name disagree"
            );
        }
        assert!(load("nope").is_none(), "unknown preset must be None");
    }
}
