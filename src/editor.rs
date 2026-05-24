use std::path::PathBuf;

pub struct Cursor {
    pub line: usize,
    pub col: usize,
}

pub struct EditorBuffer {
    pub lines: Vec<String>,
    pub cursor: Cursor,
    pub scroll_y: f32,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
}

impl EditorBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: Cursor { line: 0, col: 0 },
            scroll_y: 0.0,
            file_path: None,
            dirty: false,
        }
    }

    pub fn load_file(&mut self, path: &str) {
        let path = PathBuf::from(path);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.lines = content.lines().map(String::from).collect();
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.file_path = Some(path);
                self.cursor = Cursor { line: 0, col: 0 };
                self.dirty = false;
            }
            Err(e) => eprintln!("Cannot open file: {e}"),
        }
    }

    pub fn save(&mut self) {
        let Some(path) = self.file_path.clone() else { return };
        let content = self.lines.join("\n");
        match std::fs::write(&path, content) {
            Ok(_) => self.dirty = false,
            Err(e) => eprintln!("Cannot save: {e}"),
        }
    }

    pub fn save_to(&mut self, path: std::path::PathBuf) {
        let content = self.lines.join("\n");
        match std::fs::write(&path, content) {
            Ok(_) => {
                self.file_path = Some(path);
                self.dirty = false;
            }
            Err(e) => eprintln!("Cannot save: {e}"),
        }
    }

    pub fn insert_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.insert_char(ch);
        }
    }

    fn col_to_byte(&self, line: usize, col: usize) -> usize {
        self.lines[line]
            .char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(self.lines[line].len())
    }

    fn insert_char(&mut self, ch: char) {
        let byte = self.col_to_byte(self.cursor.line, self.cursor.col);
        self.lines[self.cursor.line].insert(byte, ch);
        self.cursor.col += 1;
        self.dirty = true;
    }

    pub fn insert_newline(&mut self) {
        let byte = self.col_to_byte(self.cursor.line, self.cursor.col);
        let tail = self.lines[self.cursor.line][byte..].to_string();
        self.lines[self.cursor.line].truncate(byte);
        self.cursor.line += 1;
        self.lines.insert(self.cursor.line, tail);
        self.cursor.col = 0;
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.cursor.col > 0 {
            let col = self.cursor.col;
            let b0 = self.col_to_byte(self.cursor.line, col - 1);
            let b1 = self.col_to_byte(self.cursor.line, col);
            self.lines[self.cursor.line].drain(b0..b1);
            self.cursor.col -= 1;
            self.dirty = true;
        } else if self.cursor.line > 0 {
            let cur = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            let prev_len = self.lines[self.cursor.line].chars().count();
            self.lines[self.cursor.line].push_str(&cur);
            self.cursor.col = prev_len;
            self.dirty = true;
        }
    }

    pub fn delete(&mut self) {
        let len = self.lines[self.cursor.line].chars().count();
        if self.cursor.col < len {
            let b0 = self.col_to_byte(self.cursor.line, self.cursor.col);
            let b1 = self.col_to_byte(self.cursor.line, self.cursor.col + 1);
            self.lines[self.cursor.line].drain(b0..b1);
            self.dirty = true;
        } else if self.cursor.line + 1 < self.lines.len() {
            let next = self.lines.remove(self.cursor.line + 1);
            self.lines[self.cursor.line].push_str(&next);
            self.dirty = true;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.lines[self.cursor.line].chars().count();
        }
    }

    pub fn move_right(&mut self) {
        let len = self.lines[self.cursor.line].chars().count();
        if self.cursor.col < len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let len = self.lines[self.cursor.line].chars().count();
            self.cursor.col = self.cursor.col.min(len);
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            let len = self.lines[self.cursor.line].chars().count();
            self.cursor.col = self.cursor.col.min(len);
        }
    }

    pub fn move_home(&mut self) {
        self.cursor.col = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor.col = self.lines[self.cursor.line].chars().count();
    }

    pub fn scroll_to_cursor(&mut self, line_height: f32, visible_h: f32) {
        let cy = self.cursor.line as f32 * line_height;
        if cy < self.scroll_y {
            self.scroll_y = cy;
        } else if cy + line_height > self.scroll_y + visible_h {
            self.scroll_y = cy + line_height - visible_h;
        }
        self.scroll_y = self.scroll_y.max(0.0);
    }
}
