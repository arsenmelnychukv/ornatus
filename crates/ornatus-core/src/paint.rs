//! Freehand brush strokes (decision 0003).
//!
//! A stroke is a list of dabs in UV space. The pointer path is turned into
//! evenly spaced dabs here, so the renderer only has to stamp them, and the
//! same dabs can be replayed for undo, redo or a different texture size.

use glam::Vec3;

/// Distance between dabs, as a fraction of the brush radius.
const SPACING: f32 = 0.2;
/// Smallest distance between dabs in UV units, so tiny brushes stay cheap.
const MIN_STEP: f32 = 0.0004;

/// How a stroke looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Brush {
    /// Color as sRGB bytes. Ignored when erasing.
    pub color: [u8; 3],
    /// Radius in UV units (a fraction of the texture width).
    pub radius_uv: f32,
    /// Strength of the whole stroke, 0 to 1. Overlapping dabs do not add up.
    pub opacity: f32,
    /// 0 for a soft edge, 1 for a hard edge.
    pub hardness: f32,
    /// Remove paint instead of adding it.
    pub erase: bool,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            color: [230, 60, 60],
            radius_uv: 12.0 / 2048.0,
            opacity: 1.0,
            hardness: 0.8,
            erase: false,
        }
    }
}

/// A finished stroke: its brush and where it was stamped.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// The brush it was drawn with.
    pub brush: Brush,
    /// Dab centers in UV space.
    pub dabs: Vec<[f32; 2]>,
}

/// What the pointer was over, as far as drawing is concerned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitSample {
    /// UV coordinate under the pointer.
    pub uv: [f32; 2],
    /// World position under the pointer.
    pub point: Vec3,
    /// How many UV units one world unit covers near this point.
    pub uv_per_world: f32,
}

/// Turns pointer samples into evenly spaced dabs.
#[derive(Debug, Clone)]
pub struct StrokeBuilder {
    brush: Brush,
    last: Option<HitSample>,
    /// Distance travelled since the last dab, in UV units.
    carry: f32,
    dabs: Vec<[f32; 2]>,
    reported: usize,
}

impl StrokeBuilder {
    /// Start a stroke with this brush.
    pub fn new(brush: Brush) -> Self {
        Self {
            brush,
            last: None,
            carry: 0.0,
            dabs: Vec::new(),
            reported: 0,
        }
    }

    fn step(&self) -> f32 {
        (self.brush.radius_uv * SPACING).max(MIN_STEP)
    }

    /// Feed the next pointer sample, or `None` when the pointer is not over
    /// the paint surface (the pen lifts, and the next sample starts a new line).
    pub fn add(&mut self, sample: Option<HitSample>) {
        let Some(next) = sample else {
            self.last = None;
            return;
        };
        let Some(previous) = self.last.replace(next) else {
            self.dabs.push(next.uv);
            self.carry = 0.0;
            return;
        };
        let from = glam::Vec2::from(previous.uv);
        let to = glam::Vec2::from(next.uv);
        let length = from.distance(to);
        // Two samples that are close in space but far apart in UV sit on
        // different islands, so they must not be joined by a line.
        let world = previous.point.distance(next.point);
        let expected = world * previous.uv_per_world.max(next.uv_per_world);
        if length > expected * 2.0 + self.step() {
            self.dabs.push(next.uv);
            self.carry = 0.0;
            return;
        }
        let step = self.step();
        let mut at = step - self.carry;
        while at <= length {
            self.dabs.push(from.lerp(to, at / length).to_array());
            at += step;
        }
        self.carry = length - (at - step);
    }

    /// Dabs added since the last call, ready to stamp.
    pub fn take_new_dabs(&mut self) -> &[[f32; 2]] {
        let start = std::mem::replace(&mut self.reported, self.dabs.len());
        &self.dabs[start..]
    }

    /// The brush this stroke started with.
    pub fn brush(&self) -> &Brush {
        &self.brush
    }

    /// True when nothing has been drawn.
    pub fn is_empty(&self) -> bool {
        self.dabs.is_empty()
    }

    /// Finish the stroke.
    pub fn finish(self) -> Stroke {
        Stroke {
            brush: self.brush,
            dabs: self.dabs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(u: f32, x: f32) -> HitSample {
        // One world unit spans one UV unit, so UV and world distance agree.
        HitSample {
            uv: [u, 0.5],
            point: Vec3::new(x, 0.0, 0.0),
            uv_per_world: 1.0,
        }
    }

    fn brush(radius_uv: f32) -> Brush {
        Brush {
            radius_uv,
            ..Brush::default()
        }
    }

    #[test]
    fn the_first_sample_stamps_one_dab() {
        let mut builder = StrokeBuilder::new(brush(0.01));
        builder.add(Some(sample(0.2, 0.2)));
        assert_eq!(builder.finish().dabs, [[0.2, 0.5]]);
    }

    #[test]
    fn a_line_gets_evenly_spaced_dabs() {
        let radius = 0.01;
        let mut builder = StrokeBuilder::new(brush(radius));
        builder.add(Some(sample(0.1, 0.1)));
        builder.add(Some(sample(0.3, 0.3)));
        let dabs = builder.finish().dabs;
        let step = radius * SPACING;
        let expected = (0.2 / step) as usize + 1;
        assert!(
            dabs.len().abs_diff(expected) <= 1,
            "{} dabs, expected about {expected}",
            dabs.len()
        );
        for pair in dabs.windows(2) {
            let gap = pair[1][0] - pair[0][0];
            assert!((gap - step).abs() < 1e-5, "gap {gap}, step {step}");
        }
    }

    #[test]
    fn spacing_carries_over_between_samples() {
        let radius = 0.01;
        let mut whole = StrokeBuilder::new(brush(radius));
        whole.add(Some(sample(0.1, 0.1)));
        whole.add(Some(sample(0.2, 0.2)));
        let mut pieces = StrokeBuilder::new(brush(radius));
        for i in 0..=10 {
            let u = 0.1 + i as f32 * 0.01;
            pieces.add(Some(sample(u, u)));
        }
        let (a, b) = (whole.finish().dabs, pieces.finish().dabs);
        assert!(a.len().abs_diff(b.len()) <= 1, "{} vs {}", a.len(), b.len());
    }

    #[test]
    fn a_jump_between_islands_is_not_joined() {
        let mut builder = StrokeBuilder::new(brush(0.01));
        builder.add(Some(sample(0.1, 0.1)));
        // Far in UV, but only 0.01 away in the world: a different island.
        builder.add(Some(HitSample {
            uv: [0.9, 0.5],
            point: Vec3::new(0.11, 0.0, 0.0),
            uv_per_world: 1.0,
        }));
        assert_eq!(builder.finish().dabs.len(), 2);
    }

    #[test]
    fn leaving_the_surface_lifts_the_pen() {
        let mut builder = StrokeBuilder::new(brush(0.01));
        builder.add(Some(sample(0.1, 0.1)));
        builder.add(None);
        builder.add(Some(sample(0.5, 0.5)));
        assert_eq!(builder.finish().dabs.len(), 2);
    }

    #[test]
    fn new_dabs_are_reported_once() {
        let mut builder = StrokeBuilder::new(brush(0.01));
        builder.add(Some(sample(0.1, 0.1)));
        assert_eq!(builder.take_new_dabs().len(), 1);
        assert!(builder.take_new_dabs().is_empty());
        builder.add(Some(sample(0.12, 0.12)));
        assert!(!builder.take_new_dabs().is_empty());
        assert!(!builder.is_empty());
    }

    #[test]
    fn a_stroke_keeps_the_brush_it_started_with() {
        let mut builder = StrokeBuilder::new(brush(0.01));
        builder.add(Some(sample(0.1, 0.1)));
        assert_eq!(builder.brush().radius_uv, 0.01);
        assert_eq!(builder.finish().brush.radius_uv, 0.01);
    }

    #[test]
    fn tiny_brushes_do_not_explode_the_dab_count() {
        let mut builder = StrokeBuilder::new(brush(0.00001));
        builder.add(Some(sample(0.0, 0.0)));
        builder.add(Some(sample(1.0, 1.0)));
        assert!(builder.finish().dabs.len() <= (1.0 / MIN_STEP) as usize + 2);
    }
}
