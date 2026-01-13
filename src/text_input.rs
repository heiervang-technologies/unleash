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
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            hidden: false,
            placeholder: String::new(),
        }
    }

    pub fn with_value(mut self, value: &str) -> Self {
        self.value = value.to_string();
        self.cursor = value.len();
        self
    }

    pub fn with_placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = placeholder.to_string();
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// Insert a character at cursor
    pub fn insert(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.value.remove(self.cursor);
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
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end
    pub fn move_end(&mut self) {
        self.cursor = self.value.len();
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
    }

    /// Delete from cursor to end of line (Ctrl+K)
    pub fn delete_to_end(&mut self) {
        self.value.truncate(self.cursor);
    }

    /// Delete from cursor to start of line (Ctrl+U)
    pub fn delete_to_start(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        self.value = chars[self.cursor..].iter().collect();
        self.cursor = 0;
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    /// Get display value (censored if hidden)
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
        || key_lower.contains("pat")
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
        assert_eq!(censor_sensitive("sk-ant-api123456789xyz", 7, 4), "sk-ant-...9xyz");
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
}
