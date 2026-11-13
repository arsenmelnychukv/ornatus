//! A bounded undo and redo stack (decision 0003).

/// Steps kept for undo before the oldest is handed back to be baked in.
pub const DEFAULT_MAX_STEPS: usize = 100;

/// Done steps and undone steps, in order.
#[derive(Debug, Clone)]
pub struct UndoStack<T> {
    done: Vec<T>,
    undone: Vec<T>,
    max_steps: usize,
}

impl<T> UndoStack<T> {
    /// An empty stack that keeps at most `max_steps` steps to undo.
    pub fn new(max_steps: usize) -> Self {
        Self {
            done: Vec::new(),
            undone: Vec::new(),
            max_steps: max_steps.max(1),
        }
    }

    /// Record a new step. This drops anything that could have been redone.
    /// When the stack is full, the oldest step is returned so the caller can
    /// bake it in, since it can no longer be undone.
    pub fn push(&mut self, step: T) -> Option<T> {
        self.undone.clear();
        self.done.push(step);
        (self.done.len() > self.max_steps).then(|| self.done.remove(0))
    }

    /// Undo the latest step. Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        match self.done.pop() {
            Some(step) => {
                self.undone.push(step);
                true
            }
            None => false,
        }
    }

    /// Redo the latest undone step. Returns false when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        match self.undone.pop() {
            Some(step) => {
                self.done.push(step);
                true
            }
            None => false,
        }
    }

    /// The steps currently applied, oldest first.
    pub fn done(&self) -> &[T] {
        &self.done
    }

    /// True when undo would do something.
    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    /// True when redo would do something.
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }
}

impl<T> Default for UndoStack<T> {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_STEPS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut stack = UndoStack::new(10);
        stack.push(1);
        stack.push(2);
        assert_eq!(stack.done(), [1, 2]);
        assert!(stack.undo());
        assert_eq!(stack.done(), [1]);
        assert!(stack.can_redo());
        assert!(stack.redo());
        assert_eq!(stack.done(), [1, 2]);
        assert!(!stack.redo());
    }

    #[test]
    fn a_new_step_drops_the_redo_branch() {
        let mut stack = UndoStack::new(10);
        stack.push(1);
        stack.push(2);
        stack.undo();
        stack.push(3);
        assert_eq!(stack.done(), [1, 3]);
        assert!(!stack.can_redo());
    }

    #[test]
    fn the_oldest_step_is_handed_back_when_full() {
        let mut stack = UndoStack::new(2);
        assert_eq!(stack.push('a'), None);
        assert_eq!(stack.push('b'), None);
        assert_eq!(stack.push('c'), Some('a'));
        assert_eq!(stack.done(), ['b', 'c']);
    }

    #[test]
    fn undoing_an_empty_stack_does_nothing() {
        let mut stack: UndoStack<u8> = UndoStack::default();
        assert!(!stack.undo() && !stack.can_undo());
        stack.push(1);
        stack.clear();
        assert!(stack.done().is_empty() && !stack.can_redo());
    }
}
