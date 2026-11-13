//! Everything the user has done to the paint surface, as one undoable history
//! (decision 0003): strokes and sticker edits share the same stack, so Ctrl+Z
//! undoes whatever came last.

use crate::paint::Stroke;
use crate::sticker::Sticker;
use crate::undo::{DEFAULT_MAX_STEPS, UndoStack};

/// One undoable step.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// A freehand stroke.
    Stroke(Stroke),
    /// A sticker was placed on top of the others.
    Add(Sticker),
    /// The sticker at `index` was removed.
    Remove { index: usize, sticker: Sticker },
    /// The sticker at `index` was changed.
    Replace {
        index: usize,
        before: Sticker,
        after: Sticker,
    },
    /// A sticker moved in the stacking order.
    Reorder { from: usize, to: usize },
}

impl Edit {
    /// Apply a sticker edit to a sticker list. Strokes do nothing here.
    fn apply(&self, stickers: &mut Vec<Sticker>) {
        match self {
            Edit::Stroke(_) => {}
            Edit::Add(sticker) => stickers.push(*sticker),
            Edit::Remove { index, .. } => {
                if *index < stickers.len() {
                    stickers.remove(*index);
                }
            }
            Edit::Replace { index, after, .. } => {
                if let Some(slot) = stickers.get_mut(*index) {
                    *slot = *after;
                }
            }
            Edit::Reorder { from, to } => {
                if *from < stickers.len() {
                    let sticker = stickers.remove(*from);
                    stickers.insert((*to).min(stickers.len()), sticker);
                }
            }
        }
    }
}

/// The history plus the stickers that are too old to undo.
#[derive(Debug, Clone)]
pub struct Document {
    /// Stickers as they were before the oldest step still in the history.
    base_stickers: Vec<Sticker>,
    history: UndoStack<Edit>,
    /// True once a stroke fell off the end of the history and was baked.
    baked_strokes: bool,
    next_sticker_id: u32,
}

impl Default for Document {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_STEPS)
    }
}

impl Document {
    /// An empty document that keeps `max_steps` steps to undo.
    pub fn new(max_steps: usize) -> Self {
        Self {
            base_stickers: Vec::new(),
            history: UndoStack::new(max_steps),
            baked_strokes: false,
            next_sticker_id: 1,
        }
    }

    /// A new sticker identity.
    pub fn next_sticker_id(&mut self) -> u32 {
        let id = self.next_sticker_id;
        self.next_sticker_id += 1;
        id
    }

    /// Record a step. When the history is full, the oldest stroke that can no
    /// longer be undone is returned so it can be baked into the paint texture.
    pub fn push(&mut self, edit: Edit) -> Option<Stroke> {
        match self.history.push(edit) {
            Some(Edit::Stroke(stroke)) => {
                self.baked_strokes = true;
                Some(stroke)
            }
            Some(other) => {
                other.apply(&mut self.base_stickers);
                None
            }
            None => None,
        }
    }

    /// Undo the latest step. Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.history.undo()
    }

    /// Redo the latest undone step. Returns false when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        self.history.redo()
    }

    /// True when undo would do something.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// True when redo would do something.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// The strokes that are applied, oldest first (not counting baked ones).
    pub fn strokes(&self) -> Vec<Stroke> {
        self.history
            .done()
            .iter()
            .filter_map(|edit| match edit {
                Edit::Stroke(stroke) => Some(stroke.clone()),
                _ => None,
            })
            .collect()
    }

    /// The stickers as they are now, bottom first.
    pub fn stickers(&self) -> Vec<Sticker> {
        let mut stickers = self.base_stickers.clone();
        for edit in self.history.done() {
            edit.apply(&mut stickers);
        }
        stickers
    }

    /// True when there is anything the user would lose by leaving.
    pub fn has_content(&self) -> bool {
        self.baked_strokes || !self.base_stickers.is_empty() || self.history.can_undo()
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        self.base_stickers.clear();
        self.history.clear();
        self.baked_strokes = false;
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;
    use crate::paint::Brush;

    fn sticker(id: u32) -> Sticker {
        Sticker {
            id,
            image: 0,
            center: Vec2::splat(0.5),
            half: Vec2::splat(0.1),
            rotation: 0.0,
            opacity: 1.0,
            flip_x: false,
        }
    }

    fn stroke() -> Stroke {
        Stroke {
            brush: Brush::default(),
            dabs: vec![[0.5, 0.5]],
        }
    }

    #[test]
    fn stickers_are_the_fold_of_the_applied_edits() {
        let mut doc = Document::default();
        doc.push(Edit::Add(sticker(1)));
        doc.push(Edit::Add(sticker(2)));
        let mut moved = sticker(2);
        moved.center = Vec2::new(0.2, 0.2);
        doc.push(Edit::Replace {
            index: 1,
            before: sticker(2),
            after: moved,
        });
        assert_eq!(doc.stickers(), [sticker(1), moved]);
        doc.push(Edit::Reorder { from: 1, to: 0 });
        assert_eq!(doc.stickers(), [moved, sticker(1)]);
        doc.push(Edit::Remove {
            index: 0,
            sticker: moved,
        });
        assert_eq!(doc.stickers(), [sticker(1)]);
    }

    #[test]
    fn undo_and_redo_walk_strokes_and_stickers_together() {
        let mut doc = Document::default();
        doc.push(Edit::Add(sticker(1)));
        doc.push(Edit::Stroke(stroke()));
        assert_eq!(doc.strokes().len(), 1);
        assert!(doc.undo());
        assert!(doc.strokes().is_empty());
        assert_eq!(doc.stickers().len(), 1);
        assert!(doc.undo());
        assert!(doc.stickers().is_empty() && !doc.can_undo());
        assert!(doc.redo() && doc.redo());
        assert_eq!(doc.stickers().len(), 1);
        assert_eq!(doc.strokes().len(), 1);
    }

    #[test]
    fn old_sticker_edits_fold_into_the_base_when_the_history_is_full() {
        let mut doc = Document::new(2);
        assert!(doc.push(Edit::Add(sticker(1))).is_none());
        assert!(doc.push(Edit::Add(sticker(2))).is_none());
        // Pushes the first Add out; the sticker must still be there.
        assert!(doc.push(Edit::Add(sticker(3))).is_none());
        assert_eq!(doc.stickers().len(), 3);
        // Only two steps can be undone.
        assert!(doc.undo() && doc.undo() && !doc.undo());
        assert_eq!(doc.stickers(), [sticker(1)]);
    }

    #[test]
    fn old_strokes_are_handed_back_for_baking() {
        let mut doc = Document::new(1);
        assert!(doc.push(Edit::Stroke(stroke())).is_none());
        assert_eq!(doc.push(Edit::Add(sticker(1))), Some(stroke()));
        assert!(doc.has_content());
        doc.clear();
        assert!(!doc.has_content() && doc.stickers().is_empty());
    }

    #[test]
    fn a_new_edit_drops_the_redo_branch() {
        let mut doc = Document::default();
        doc.push(Edit::Add(sticker(1)));
        doc.undo();
        doc.push(Edit::Add(sticker(2)));
        assert!(!doc.can_redo());
        assert_eq!(doc.stickers(), [sticker(2)]);
    }

    #[test]
    fn sticker_ids_are_unique() {
        let mut doc = Document::default();
        assert_ne!(doc.next_sticker_id(), doc.next_sticker_id());
    }
}
