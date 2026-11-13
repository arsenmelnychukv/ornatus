//! Picking a sensible default paint surface by name.
//!
//! Names are only hints. The user always makes the final choice from a list.

use crate::model::Model;

const GOOD: [(&str, i32); 5] = [
    ("outer", 3),
    ("lid", 2),
    ("cover", 2),
    ("top", 1),
    ("shell", 1),
];
const BAD: [&str; 14] = [
    "screen", "display", "panel", "key", "keys", "keyboard", "hinge", "base", "bottom", "inner",
    "inside", "bezel", "camera", "port",
];

/// Split a name into lowercase words at punctuation, spaces and camelCase
/// boundaries, so "LidOuter" and "lid_outer" both give ["lid", "outer"].
fn words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for ch in name.chars() {
        if !ch.is_alphanumeric() {
            words.push(std::mem::take(&mut current));
            previous_lower = false;
            continue;
        }
        if ch.is_uppercase() && previous_lower {
            words.push(std::mem::take(&mut current));
        }
        current.extend(ch.to_lowercase());
        previous_lower = ch.is_lowercase();
    }
    words.push(current);
    words.retain(|w| !w.is_empty());
    words
}

fn score(name: &str) -> i32 {
    words(name)
        .iter()
        .map(|word| {
            let good = GOOD
                .iter()
                .find(|(w, _)| w == word)
                .map_or(0, |(_, points)| *points);
            let bad = i32::from(BAD.contains(&word.as_str()));
            good - 3 * bad
        })
        .sum()
}

/// Index of the surface that looks most like an outer lid, if any does.
pub fn suggest_surface(model: &Model) -> Option<usize> {
    model
        .meshes
        .iter()
        .enumerate()
        .map(|(i, mesh)| {
            (
                i,
                score(&mesh.name) + mesh.material.as_deref().map_or(0, score),
            )
        })
        .filter(|&(_, s)| s > 0)
        .max_by_key(|&(i, s)| (s, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::load_glb;

    #[test]
    fn suggests_the_outer_lid_of_the_default_laptop() {
        let model = load_glb(&ornatus_fixtures::laptop_glb()).unwrap().model;
        let index = suggest_surface(&model).expect("a suggestion");
        assert_eq!(model.meshes[index].name, "Lid_Outer");
    }

    #[test]
    fn suggests_nothing_without_hints() {
        let model = load_glb(&ornatus_fixtures::grid_glb(4)).unwrap().model;
        assert_eq!(suggest_surface(&model), None);
    }

    #[test]
    fn whole_words_only() {
        // "laptop" contains "top", "lidar" contains "lid", "keyboard" is not "key".
        assert_eq!(score("Laptop"), 0);
        assert_eq!(score("Lidar_Mount"), 0);
        assert!(score("Keyboard") < 0);
        assert_eq!(words("LidOuter"), ["lid", "outer"]);
        assert_eq!(words("A_cover.001"), ["a", "cover", "001"]);
    }

    #[test]
    fn screens_and_bases_score_low() {
        assert!(score("Lid_Screen") <= 0);
        assert!(score("Base") <= 0);
        assert!(score("Lid_Outer") > 0);
        assert!(score("A_cover") > 0);
    }
}
