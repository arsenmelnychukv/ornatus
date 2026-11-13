//! Scene, paint, sticker, picking and undo logic for ornatus.
//!
//! This crate has no GPU, window or browser code, so everything in it is
//! unit-testable on any machine.

use std::fmt;

/// The tool the user is currently working with (see decision 0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Place a new sticker with a ghost preview.
    #[default]
    Place,
    /// Select and edit a placed sticker.
    Select,
    /// Draw freehand strokes.
    Draw,
}

impl Mode {
    /// Every mode, in the order shown in the UI.
    pub const ALL: [Mode; 3] = [Mode::Place, Mode::Select, Mode::Draw];
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Mode::Place => "Place",
            Mode::Select => "Select",
            Mode::Draw => "Draw",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mode_is_place() {
        assert_eq!(Mode::default(), Mode::Place);
    }

    #[test]
    fn all_modes_have_distinct_labels() {
        let labels: Vec<String> = Mode::ALL.iter().map(ToString::to_string).collect();
        assert_eq!(labels, ["Place", "Select", "Draw"]);
    }
}
