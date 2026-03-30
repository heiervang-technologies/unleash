//! Input handling with vim + arrow key navigation

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Navigation actions that can be performed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Up,
    Down,
    Left,
    Right,
    Select,
    Back,
    Quit,
    Delete,
    Edit,
    New,
    Help,
    Tab,
    BackTab,
    ExternalEdit,
    None,
}

/// Convert a key event to a navigation action
pub fn key_to_action(key: KeyEvent) -> NavAction {
    match key.code {
        // Vim navigation
        KeyCode::Char('j') => NavAction::Down,
        KeyCode::Char('k') => NavAction::Up,
        KeyCode::Char('h') => NavAction::Left,
        KeyCode::Char('l') => NavAction::Right,

        // Arrow keys
        KeyCode::Up => NavAction::Up,
        KeyCode::Down => NavAction::Down,
        KeyCode::Left => NavAction::Left,
        KeyCode::Right => NavAction::Right,

        // Selection
        KeyCode::Enter => NavAction::Select,
        KeyCode::Char(' ') => NavAction::Select,

        // Back/Cancel
        KeyCode::Esc => NavAction::Back,
        KeyCode::Backspace => NavAction::Back,

        // Quit
        KeyCode::Char('q') => NavAction::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => NavAction::Quit,

        // Tab / Shift+Tab
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                NavAction::BackTab
            } else {
                NavAction::Tab
            }
        }
        KeyCode::BackTab => NavAction::BackTab,

        // Actions
        KeyCode::Char('d') => NavAction::Delete,
        KeyCode::Char('e') => NavAction::Edit,
        KeyCode::Char('n') => NavAction::New,
        KeyCode::Char('o') => NavAction::ExternalEdit,
        KeyCode::Char('?') => NavAction::Help,

        _ => NavAction::None,
    }
}

/// A simple menu state manager with scroll support
#[derive(Debug, Clone)]
pub struct MenuState {
    pub selected: usize,
    pub items_count: usize,
    /// Scroll offset for when items exceed viewport
    pub scroll_offset: usize,
}

impl MenuState {
    pub fn new(items_count: usize) -> Self {
        Self {
            selected: 0,
            items_count,
            scroll_offset: 0,
        }
    }

    pub fn select_next(&mut self) {
        if self.items_count > 0 {
            self.selected = (self.selected + 1).min(self.items_count - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self) {
        if self.items_count > 0 {
            self.selected = self.items_count - 1;
        }
    }

    pub fn set_items_count(&mut self, count: usize) {
        self.items_count = count;
        if self.selected >= count && count > 0 {
            self.selected = count - 1;
        }
        // Reset scroll if items reduced
        if self.scroll_offset >= count {
            self.scroll_offset = 0;
        }
    }

    /// Adjust scroll offset to ensure selected item is visible
    /// Call this after changing selection and before rendering
    /// `visible_count` is how many items fit in the viewport
    pub fn ensure_visible(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }

        // If selected is above visible area, scroll up
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }

        // If selected is below visible area, scroll down
        if self.selected >= self.scroll_offset + visible_count {
            self.scroll_offset = self.selected.saturating_sub(visible_count - 1);
        }
    }

    /// Get the range of items to display
    /// Returns (start_index, items_to_show)
    #[allow(dead_code)]
    pub fn visible_range(&self, visible_count: usize) -> (usize, usize) {
        let start = self.scroll_offset;
        let count = visible_count.min(self.items_count.saturating_sub(start));
        (start, count)
    }

    /// Handle a navigation action, returns true if handled
    pub fn handle_action(&mut self, action: NavAction) -> bool {
        match action {
            NavAction::Up => {
                self.select_prev();
                true
            }
            NavAction::Down => {
                self.select_next();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_navigation() {
        let mut menu = MenuState::new(3);
        assert_eq!(menu.selected, 0);

        menu.select_next();
        assert_eq!(menu.selected, 1);

        menu.select_next();
        assert_eq!(menu.selected, 2);

        // Clamp at bottom (no wrap)
        menu.select_next();
        assert_eq!(menu.selected, 2);

        // Go back
        menu.select_prev();
        assert_eq!(menu.selected, 1);

        // Clamp at top (no wrap)
        menu.selected = 0;
        menu.select_prev();
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_vim_keys() {
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
            NavAction::Down
        );
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)),
            NavAction::Up
        );
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)),
            NavAction::Left
        );
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE)),
            NavAction::Right
        );
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            NavAction::Up
        );
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            NavAction::Down
        );
    }

    #[test]
    fn test_action_keys() {
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            NavAction::Select
        );
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            NavAction::Back
        );
        assert_eq!(
            key_to_action(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            NavAction::Quit
        );
    }

    #[test]
    fn test_handle_action() {
        let mut menu = MenuState::new(3);

        assert!(menu.handle_action(NavAction::Down));
        assert_eq!(menu.selected, 1);

        assert!(menu.handle_action(NavAction::Up));
        assert_eq!(menu.selected, 0);

        // Non-navigation actions return false
        assert!(!menu.handle_action(NavAction::Select));
        assert!(!menu.handle_action(NavAction::Quit));
    }

    #[test]
    fn test_empty_menu() {
        let mut menu = MenuState::new(0);
        menu.select_next(); // Should not panic
        menu.select_prev(); // Should not panic
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_select_first_and_last() {
        let mut menu = MenuState::new(5);
        menu.selected = 3;

        menu.select_first();
        assert_eq!(menu.selected, 0);

        menu.select_last();
        assert_eq!(menu.selected, 4);
    }

    #[test]
    fn test_set_items_count() {
        let mut menu = MenuState::new(5);
        menu.selected = 4;

        // Shrink - should clamp selected
        menu.set_items_count(2);
        assert_eq!(menu.selected, 1);

        // Expand - selected stays
        menu.set_items_count(10);
        assert_eq!(menu.selected, 1);
    }
}
