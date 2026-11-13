//! Lighting presets and viewport backgrounds (Phase 8).

/// How the studio around the model is lit. Every value scales the default
/// studio, so `Environment::STUDIO` changes nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Shown in the UI.
    pub name: &'static str,
    /// Overall brightness before tone mapping.
    pub exposure: f32,
    /// Strength of the main soft box.
    pub key: f32,
    /// Strength of the fill soft box.
    pub fill: f32,
    /// Strength of the rim soft box behind the model.
    pub rim: f32,
    /// Strength of the room's base glow.
    pub ambient: f32,
}

impl Environment {
    /// The default look.
    pub const STUDIO: Environment = Environment {
        name: "Studio",
        exposure: 1.0,
        key: 1.0,
        fill: 1.0,
        rim: 1.0,
        ambient: 1.0,
    };
}

/// All lighting presets, default first.
pub const ENVIRONMENTS: [Environment; 3] = [
    Environment::STUDIO,
    Environment {
        name: "Bright",
        exposure: 1.3,
        key: 0.7,
        fill: 1.3,
        rim: 0.6,
        ambient: 1.6,
    },
    Environment {
        name: "Dramatic",
        exposure: 0.9,
        key: 1.7,
        fill: 0.25,
        rim: 1.8,
        ambient: 0.4,
    },
];

/// A background color for the viewport. Screenshots stay transparent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Background {
    /// Shown in the UI.
    pub name: &'static str,
    /// sRGB color.
    pub color: [u8; 3],
}

/// All viewport backgrounds, default first.
pub const BACKGROUNDS: [Background; 3] = [
    Background {
        name: "Dark",
        color: [18, 18, 22],
    },
    Background {
        name: "Gray",
        color: [74, 78, 88],
    },
    Background {
        name: "Light",
        color: [226, 229, 236],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_environment_is_the_default_studio() {
        assert_eq!(ENVIRONMENTS[0], Environment::STUDIO);
        assert_eq!(Environment::STUDIO.exposure, 1.0);
    }

    #[test]
    fn names_are_unique_and_values_are_sensible() {
        let names: Vec<_> = ENVIRONMENTS.iter().map(|e| e.name).collect();
        for (i, name) in names.iter().enumerate() {
            assert!(!names[i + 1..].contains(name), "duplicate {name}");
        }
        for env in ENVIRONMENTS {
            assert!(env.exposure > 0.0 && env.ambient >= 0.0);
            assert!(env.key >= 0.0 && env.fill >= 0.0 && env.rim >= 0.0);
        }
        assert_ne!(BACKGROUNDS[0].name, BACKGROUNDS[1].name);
    }
}
