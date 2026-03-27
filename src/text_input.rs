//! Text input component with optional hidden mode

/// A text input field
#[derive(Debug, Clone)]
pub struct TextInput {
    /// Current input value
    pub value: String,
    /// Cursor position
    pub cursor: usize,
    /// Whether input is hidden (for passwords/keys)
    pub hidden: bool,
    /// Placeholder text
    pub placeholder: String,
    /// Scroll offset (first visible character index) for viewport scrolling
    pub scroll_offset: usize,
    /// Viewport width (max visible characters)
    pub viewport_width: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            hidden: false,
            placeholder: String::new(),
            scroll_offset: 0,
            viewport_width: 60, // Default viewport width
        }
    }

    pub fn with_value(mut self, value: &str) -> Self {
        self.value = value.to_string();
        self.cursor = value.len();
        self.ensure_cursor_visible();
        self
    }

    pub fn with_placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = placeholder.to_string();
        self
    }

    #[allow(dead_code)]
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// Insert a character at cursor
    pub fn insert(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += 1;
        self.ensure_cursor_visible();
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.value.remove(self.cursor);
            self.ensure_cursor_visible();
        }
    }

    /// Delete character at cursor (delete key)
    pub fn delete(&mut self) {
        if self.cursor < self.value.len() {
            self.value.remove(self.cursor);
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor += 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor to start
    pub fn move_home(&mut self) {
        self.cursor = 0;
        self.ensure_cursor_visible();
    }

    /// Move cursor to end
    pub fn move_end(&mut self) {
        self.cursor = self.value.len();
        self.ensure_cursor_visible();
    }

    /// Move cursor to previous word boundary
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.value.chars().collect();
        let mut pos = self.cursor - 1;

        // Skip any whitespace/punctuation before cursor
        while pos > 0 && !chars[pos].is_alphanumeric() {
            pos -= 1;
        }
        // Move to start of word
        while pos > 0 && chars[pos - 1].is_alphanumeric() {
            pos -= 1;
        }
        self.cursor = pos;
        self.ensure_cursor_visible();
    }

    /// Move cursor to next word boundary
    pub fn move_word_right(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let len = chars.len();
        if self.cursor >= len {
            return;
        }
        let mut pos = self.cursor;

        // Skip current word
        while pos < len && chars[pos].is_alphanumeric() {
            pos += 1;
        }
        // Skip whitespace/punctuation
        while pos < len && !chars[pos].is_alphanumeric() {
            pos += 1;
        }
        self.cursor = pos;
        self.ensure_cursor_visible();
    }

    /// Delete word before cursor (Ctrl+W)
    pub fn delete_word_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let old_cursor = self.cursor;
        self.move_word_left();
        // Remove characters between new cursor and old cursor
        let chars: Vec<char> = self.value.chars().collect();
        self.value = chars[..self.cursor]
            .iter()
            .chain(chars[old_cursor..].iter())
            .collect();
        self.ensure_cursor_visible();
    }

    /// Delete from cursor to end of line (Ctrl+K)
    pub fn delete_to_end(&mut self) {
        self.value.truncate(self.cursor);
        self.ensure_cursor_visible();
    }

    /// Delete from cursor to start of line (Ctrl+U)
    pub fn delete_to_start(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        self.value = chars[self.cursor..].iter().collect();
        self.cursor = 0;
        self.scroll_offset = 0;
    }

    /// Set the viewport width for scrolling
    #[allow(dead_code)]
    pub fn set_viewport_width(&mut self, width: usize) {
        self.viewport_width = width.max(10); // Minimum width of 10
        self.ensure_cursor_visible();
    }

    /// Ensure cursor is visible within the viewport
    pub fn ensure_cursor_visible(&mut self) {
        if self.viewport_width == 0 {
            return;
        }

        // If cursor is before viewport, scroll left
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        }

        // If cursor is beyond viewport, scroll right
        // Leave 1 char margin for the cursor indicator
        let effective_width = self.viewport_width.saturating_sub(1);
        if self.cursor >= self.scroll_offset + effective_width {
            self.scroll_offset = self.cursor.saturating_sub(effective_width) + 1;
        }
    }

    /// Get the visible portion of the value within the viewport
    #[allow(dead_code)]
    pub fn visible_value(&self) -> String {
        if self.value.is_empty() {
            return String::new();
        }

        let chars: Vec<char> = self.value.chars().collect();
        let start = self.scroll_offset.min(chars.len());
        let end = (self.scroll_offset + self.viewport_width).min(chars.len());

        chars[start..end].iter().collect()
    }

    /// Get cursor position within the visible viewport
    #[allow(dead_code)]
    pub fn visible_cursor_position(&self) -> usize {
        self.cursor.saturating_sub(self.scroll_offset)
    }

    /// Check if there's content scrolled off to the left
    #[allow(dead_code)]
    pub fn has_left_overflow(&self) -> bool {
        self.scroll_offset > 0
    }

    /// Check if there's content scrolled off to the right
    #[allow(dead_code)]
    pub fn has_right_overflow(&self) -> bool {
        self.value.len() > self.scroll_offset + self.viewport_width
    }

    /// Get text split at cursor position for rendering.
    ///
    /// Returns `(before_cursor, char_at_cursor, after_cursor)` within the visible viewport.
    /// If the cursor is at the end of text, `char_at_cursor` is `None` — the caller
    /// should render a block cursor indicator there.
    ///
    /// For hidden fields, characters are replaced with `'*'`.
    pub fn render_parts(&self) -> (String, Option<char>, String) {
        let display_chars: Vec<char> = if self.hidden {
            vec!['*'; self.value.len()]
        } else {
            self.value.chars().collect()
        };

        let start = self.scroll_offset.min(display_chars.len());
        let end = (self.scroll_offset + self.viewport_width).min(display_chars.len());
        let visible = &display_chars[start..end];
        let cursor_pos = self.cursor.saturating_sub(self.scroll_offset);

        let before: String = visible[..cursor_pos.min(visible.len())].iter().collect();
        let at_cursor = if cursor_pos < visible.len() {
            Some(visible[cursor_pos])
        } else {
            None
        };
        let after: String = if cursor_pos + 1 < visible.len() {
            visible[cursor_pos + 1..].iter().collect()
        } else {
            String::new()
        };

        (before, at_cursor, after)
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.scroll_offset = 0;
    }

    /// Get display value (censored if hidden)
    #[allow(dead_code)]
    pub fn display_value(&self) -> String {
        if self.value.is_empty() {
            return self.placeholder.clone();
        }
        if self.hidden {
            "*".repeat(self.value.len())
        } else {
            self.value.clone()
        }
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Censor a sensitive value, showing prefix and suffix
/// e.g., "sk-ant-api123456789xyz" -> "sk-ant-...9xyz"
pub fn censor_sensitive(value: &str, prefix_len: usize, suffix_len: usize) -> String {
    if value.len() <= prefix_len + suffix_len + 3 {
        // Too short to meaningfully censor - use fixed length to hide actual length
        return "*".repeat(8);
    }

    let prefix: String = value.chars().take(prefix_len).collect();
    let suffix: String = value.chars().skip(value.len() - suffix_len).collect();
    format!("{}...{}", prefix, suffix)
}

/// Check if a key name suggests it's sensitive
pub fn is_sensitive_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    key_lower.contains("key")
        || key_lower.contains("secret")
        || key_lower.contains("token")
        || key_lower.contains("password")
        || key_lower == "pat"
        || key_lower.ends_with("_pat")
        || key_lower.starts_with("pat_")
        || key_lower.contains("credential")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_input_basic() {
        let mut input = TextInput::new();
        assert!(input.is_empty());

        input.insert('h');
        input.insert('i');
        assert_eq!(input.value, "hi");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_text_input_backspace() {
        let mut input = TextInput::new().with_value("hello");
        input.backspace();
        assert_eq!(input.value, "hell");
        assert_eq!(input.cursor, 4);
    }

    #[test]
    fn test_text_input_cursor_movement() {
        let mut input = TextInput::new().with_value("hello");
        assert_eq!(input.cursor, 5);

        input.move_left();
        assert_eq!(input.cursor, 4);

        input.move_home();
        assert_eq!(input.cursor, 0);

        input.move_end();
        assert_eq!(input.cursor, 5);
    }

    #[test]
    fn test_hidden_display() {
        let input = TextInput::new().with_value("secret123").hidden();
        assert_eq!(input.display_value(), "*********");
    }

    #[test]
    fn test_censor_sensitive() {
        assert_eq!(
            censor_sensitive("sk-ant-api123456789xyz", 7, 4),
            "sk-ant-...9xyz"
        );
        assert_eq!(censor_sensitive("short", 7, 4), "********"); // Too short
        assert_eq!(censor_sensitive("abcdefghijklmnop", 4, 4), "abcd...mnop");
    }

    #[test]
    fn test_is_sensitive_key() {
        assert!(is_sensitive_key("ANTHROPIC_API_KEY"));
        assert!(is_sensitive_key("SECRET_TOKEN"));
        assert!(is_sensitive_key("MY_PASSWORD"));
        assert!(is_sensitive_key("GITHUB_PAT"));
        assert!(!is_sensitive_key("ANTHROPIC_BASE_URL"));
        assert!(!is_sensitive_key("HOME"));
    }

    #[test]
    fn test_word_navigation() {
        let mut input = TextInput::new().with_value("hello world test");
        assert_eq!(input.cursor, 16);

        input.move_word_left();
        assert_eq!(input.cursor, 12); // before "test"

        input.move_word_left();
        assert_eq!(input.cursor, 6); // before "world"

        input.move_word_left();
        assert_eq!(input.cursor, 0); // before "hello"

        input.move_word_right();
        assert_eq!(input.cursor, 6); // after "hello "

        input.move_word_right();
        assert_eq!(input.cursor, 12); // after "world "
    }

    #[test]
    fn test_delete_word_back() {
        let mut input = TextInput::new().with_value("hello world");
        input.delete_word_back();
        assert_eq!(input.value, "hello ");
        assert_eq!(input.cursor, 6);
    }

    #[test]
    fn test_delete_to_end() {
        let mut input = TextInput::new().with_value("hello world");
        input.cursor = 6;
        input.delete_to_end();
        assert_eq!(input.value, "hello ");
    }

    #[test]
    fn test_delete_to_start() {
        let mut input = TextInput::new().with_value("hello world");
        input.cursor = 6;
        input.delete_to_start();
        assert_eq!(input.value, "world");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_render_parts_cursor_at_end() {
        let input = TextInput::new().with_value("hello");
        let (before, at_cursor, after) = input.render_parts();
        assert_eq!(before, "hello");
        assert_eq!(at_cursor, None);
        assert_eq!(after, "");
    }

    #[test]
    fn test_render_parts_cursor_at_start() {
        let mut input = TextInput::new().with_value("hello");
        input.cursor = 0;
        let (before, at_cursor, after) = input.render_parts();
        assert_eq!(before, "");
        assert_eq!(at_cursor, Some('h'));
        assert_eq!(after, "ello");
    }

    #[test]
    fn test_render_parts_cursor_in_middle() {
        let mut input = TextInput::new().with_value("hello");
        input.cursor = 2;
        let (before, at_cursor, after) = input.render_parts();
        assert_eq!(before, "he");
        assert_eq!(at_cursor, Some('l'));
        assert_eq!(after, "lo");
    }

    #[test]
    fn test_is_sensitive_key_pat_not_path() {
        // PAT (Personal Access Token) variants should be sensitive
        assert!(is_sensitive_key("GITHUB_PAT"));
        assert!(is_sensitive_key("GH_PAT"));
        assert!(is_sensitive_key("PAT"));
        // PATH variants should NOT be sensitive
        assert!(!is_sensitive_key("PATH"));
        assert!(!is_sensitive_key("GOPATH"));
        assert!(!is_sensitive_key("CLASSPATH"));
    }

    #[test]
    fn test_render_parts_empty() {
        let input = TextInput::new();
        let (before, at_cursor, after) = input.render_parts();
        assert_eq!(before, "");
        assert_eq!(at_cursor, None);
        assert_eq!(after, "");
    }

    #[test]
    fn test_render_parts_hidden() {
        let mut input = TextInput::new().with_value("secret").hidden();
        input.cursor = 3;
        let (before, at_cursor, after) = input.render_parts();
        assert_eq!(before, "***");
        assert_eq!(at_cursor, Some('*'));
        assert_eq!(after, "**");
    }
}
