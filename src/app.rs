mod event_handler;
mod git;
mod lsp;
mod printer;
mod syntax;
mod tasks;
mod terminal;
mod ui;
mod workspace;

use lsp::{LspClient, LspMessage};
use printer::{TextFormat, TextPrinter};
use std::collections::VecDeque;
use std::path::PathBuf;
use tasks::{TaskEvent, TaskKind, TaskRunner};
use terminal::TerminalPane;
use workspace::{EditorBuffer, Explorer};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Normal,
    Insert,
    Visual,
    Search,
    ExCommand,
    HelpMenu,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActivePanel {
    Editor,
    Explorer,
    Terminal,
    Output,
    Problems,
    Git,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Other,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspRequestKind {
    Completion,
    Definition,
    Hover,
}

#[derive(Debug, Clone)]
pub struct PendingLsp {
    pub id: u64,
    pub kind: LspRequestKind,
}

pub struct App {
    pub running: bool,
    pub mode: AppMode,
    pub active_panel: ActivePanel,
    pub printer: TextPrinter,
    pub text_buffer: Vec<String>,
    pub cursor: (usize, usize),
    pub status_message: String,
    pub error_message: Option<String>,
    pub search_query: String,
    pub search_results: Vec<(usize, usize)>,
    pub current_search_result: usize,
    pub command_history: VecDeque<String>,
    pub ex_command: String,
    pub file_name: Option<String>,
    pub modified: bool,
    pub word_wrap: bool,
    pub show_line_numbers: bool,
    pub print_history: VecDeque<String>,
    pub char_count: usize,
    pub word_count: usize,
    pub line_count: usize,
    pub tab_size: usize,
    pub auto_indent: bool,
    pub show_mode_in_status: bool,
    pub visual_start: (usize, usize),
    pub yank_buffer: Vec<String>,
    pub scroll_offset: usize,
    pub trailing_newline: bool,

    pub explorer: Explorer,
    pub buffers: Vec<EditorBuffer>,
    pub active_tab: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub tasks: TaskRunner,
    pub task_output: String,
    pub task_title: String,
    pub terminal: TerminalPane,
    pub git_branch: String,
    pub git_status: Vec<String>,
    pub lsp: Option<LspClient>,
    pub lsp_error: Option<String>,
    pub pending_lsp: Option<PendingLsp>,
    pub completions: Vec<String>,
    pub completion_index: usize,
    pub problem_index: usize,
}

pub(crate) fn floor_char_boundary(s: &str, idx: usize) -> usize {
    let mut idx = idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, idx: usize) -> usize {
    let mut idx = idx.min(s.len());
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

#[allow(dead_code)]
fn char_starts(s: &str) -> Vec<usize> {
    s.char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(s.len()))
        .collect()
}

#[allow(dead_code)]
fn char_index(starts: &[usize], byte_col: usize) -> usize {
    starts.partition_point(|&b| b <= byte_col).saturating_sub(1)
}

fn parse_substitute(rest: &str) -> Option<(&str, &str, bool)> {
    let mut parts = rest.rsplitn(3, '/');
    let flags = parts.next()?;
    let replacement = parts.next()?;
    let pattern = parts.next()?;
    if pattern.is_empty() {
        return None;
    }
    Some((pattern, replacement, flags.contains('g')))
}

impl App {
    pub fn new() -> Self {
        let mut printer = TextPrinter::new();
        printer.set_format(TextFormat {
            bold: true,
            color: Some(ratatui::style::Color::Cyan),
            ..Default::default()
        });

        let welcome_text = vec![String::new()];

        let line_count = welcome_text.len();
        let word_count = welcome_text
            .iter()
            .map(|l| l.split_whitespace().count())
            .sum();
        let char_count = welcome_text.iter().map(|l| l.chars().count()).sum();

        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let (git_branch, git_status) = git::status(&root);

        Self {
            running: true,
            mode: AppMode::Insert,
            active_panel: ActivePanel::Editor,
            printer,
            text_buffer: welcome_text,
            cursor: (0, 0),
            status_message: String::new(),
            error_message: None,
            search_query: String::new(),
            search_results: Vec::new(),
            current_search_result: 0,
            command_history: VecDeque::new(),
            ex_command: String::new(),
            file_name: None,
            modified: false,
            word_wrap: true,
            show_line_numbers: true,
            print_history: VecDeque::new(),
            char_count,
            word_count,
            line_count,
            tab_size: 4,
            auto_indent: true,
            show_mode_in_status: true,
            visual_start: (0, 0),
            yank_buffer: Vec::new(),
            scroll_offset: 0,
            trailing_newline: true,

            explorer: Explorer::new(root),
            buffers: vec![EditorBuffer::untitled()],
            active_tab: 0,
            diagnostics: Vec::new(),
            tasks: TaskRunner::new(),
            task_output: String::new(),
            task_title: "No task has been run yet".to_string(),
            terminal: TerminalPane::new(),
            git_branch,
            git_status,
            lsp: None,
            lsp_error: None,
            pending_lsp: None,
            completions: Vec::new(),
            completion_index: 0,
            problem_index: 0,
        }
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = message.into();
    }

    pub fn set_status_error(&mut self, message: impl Into<String>) {
        self.error_message = Some(message.into());
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    pub fn update_counts(&mut self) {
        self.line_count = self.text_buffer.len();
        self.word_count = self
            .text_buffer
            .iter()
            .map(|l| l.split_whitespace().count())
            .sum();
        self.char_count = self.text_buffer.iter().map(|l| l.chars().count()).sum();
    }

    pub fn insert_at_cursor(&mut self, ch: char) {
        let (line, col) = self.cursor;
        if line < self.text_buffer.len() {
            let line_str = &mut self.text_buffer[line];
            let col = floor_char_boundary(line_str, col);
            line_str.insert(col, ch);
            self.cursor.1 = col + ch.len_utf8();
            self.modified = true;
            self.update_counts();
        } else {
            self.text_buffer.push(String::from(ch));
            self.cursor = (line, ch.len_utf8());
            self.modified = true;
            self.update_counts();
        }
    }

    pub fn insert_newline(&mut self) {
        let (line, col) = self.cursor;
        if line < self.text_buffer.len() {
            let current = self.text_buffer[line].clone();
            let col = floor_char_boundary(&current, col);
            let rest = current[col..].to_string();
            self.text_buffer[line] = current[..col].to_string();
            self.text_buffer.insert(line + 1, rest);
            self.cursor = (line + 1, 0);
            if self.auto_indent {
                let indent = self.get_indent(&current);
                self.text_buffer[line + 1].insert_str(0, &indent);
                self.cursor.1 = indent.len();
            }
        } else {
            self.text_buffer.push(String::new());
            self.cursor = (line, 0);
        }
        self.modified = true;
        self.update_counts();
    }

    fn get_indent(&self, line: &str) -> String {
        let mut indent = String::new();
        for ch in line.chars() {
            if ch == ' ' || ch == '\t' {
                indent.push(ch);
            } else {
                break;
            }
        }
        indent
    }

    #[allow(dead_code)]
    pub fn open_new_line(&mut self, below: bool) {
        let line = self.cursor.0.min(self.text_buffer.len().saturating_sub(1));
        let indent = if self.auto_indent && line < self.text_buffer.len() {
            self.get_indent(&self.text_buffer[line])
        } else {
            String::new()
        };
        if below {
            self.text_buffer.insert(line + 1, indent.clone());
            self.cursor = (line + 1, indent.len());
        } else {
            self.text_buffer.insert(line, indent.clone());
            self.cursor = (line, indent.len());
        }
        self.modified = true;
        self.update_counts();
    }

    pub fn delete_at_cursor(&mut self) {
        let (line, col) = self.cursor;
        if line >= self.text_buffer.len() {
            return;
        }
        let col = floor_char_boundary(&self.text_buffer[line], col);
        if col < self.text_buffer[line].len() {
            self.text_buffer[line].remove(col);
            self.modified = true;
            self.update_counts();
        } else if line + 1 < self.text_buffer.len() {
            let next = self.text_buffer.remove(line + 1);
            self.text_buffer[line].push_str(&next);
            self.modified = true;
            self.update_counts();
        }
    }

    pub fn delete_backward(&mut self) {
        let (line, col) = self.cursor;
        if line >= self.text_buffer.len() {
            return;
        }
        if col > 0 {
            let prev = floor_char_boundary(&self.text_buffer[line], col - 1);
            self.text_buffer[line].remove(prev);
            self.cursor.1 = prev;
            self.modified = true;
            self.update_counts();
        } else if line > 0 {
            let prev = self.text_buffer[line - 1].clone();
            let current = self.text_buffer[line].clone();
            self.text_buffer.remove(line);
            self.text_buffer[line - 1] = format!("{}{}", prev, current);
            self.cursor.1 = prev.len();
            self.cursor.0 -= 1;
            self.modified = true;
            self.update_counts();
        }
    }

    pub fn move_cursor_up(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.cursor.1 = self.clamp_column(self.cursor.1);
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.cursor.0 + 1 < self.text_buffer.len() {
            self.cursor.0 += 1;
            self.cursor.1 = self.clamp_column(self.cursor.1);
        }
    }

    pub fn move_cursor_left(&mut self) {
        let (line, col) = self.cursor;
        if col > 0 {
            if line < self.text_buffer.len() {
                self.cursor.1 = floor_char_boundary(&self.text_buffer[line], col - 1);
            }
        } else if line > 0 {
            self.cursor.0 -= 1;
            self.cursor.1 = self.text_buffer[self.cursor.0].len();
        }
    }

    pub fn move_cursor_right(&mut self) {
        let (line, col) = self.cursor;
        if line >= self.text_buffer.len() {
            return;
        }
        let line_str = &self.text_buffer[line];
        if col < line_str.len() {
            self.cursor.1 = ceil_char_boundary(line_str, col + 1);
        } else if line + 1 < self.text_buffer.len() {
            self.cursor.0 += 1;
            self.cursor.1 = 0;
        }
    }

    pub fn move_cursor_to_line_start(&mut self) {
        self.cursor.1 = 0;
    }

    pub fn move_cursor_to_line_end(&mut self) {
        if self.cursor.0 < self.text_buffer.len() {
            self.cursor.1 = self.text_buffer[self.cursor.0].len();
        }
    }

    #[allow(dead_code)]
    pub fn move_cursor_to_start(&mut self) {
        self.cursor = (0, 0);
    }

    #[allow(dead_code)]
    pub fn move_cursor_to_end(&mut self) {
        if let Some(last_line) = self.text_buffer.len().checked_sub(1) {
            self.cursor = (last_line, self.text_buffer[last_line].len());
        }
    }

    pub fn move_to_first_non_blank(&mut self) {
        if self.cursor.0 < self.text_buffer.len() {
            let line = &self.text_buffer[self.cursor.0];
            if let Some((i, _)) = line.char_indices().find(|(_, ch)| !ch.is_whitespace()) {
                self.cursor.1 = i;
            } else {
                self.cursor.1 = line.len();
            }
        }
    }

    #[allow(dead_code)]
    pub fn move_word_forward(&mut self) {
        if self.text_buffer.is_empty() {
            return;
        }
        let mut line = self.cursor.0.min(self.text_buffer.len() - 1);
        let mut col = self.cursor.1;
        loop {
            let line_str = &self.text_buffer[line];
            let starts = char_starts(line_str);
            let chars: Vec<char> = line_str.chars().collect();
            let mut pos = char_index(&starts, col);

            if pos < chars.len() && !chars[pos].is_whitespace() {
                while pos < chars.len() && !chars[pos].is_whitespace() {
                    pos += 1;
                }
            }
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            if pos < chars.len() {
                self.cursor = (line, starts[pos]);
                return;
            }
            if line + 1 >= self.text_buffer.len() {
                self.cursor = (line, line_str.len());
                return;
            }
            line += 1;
            col = 0;
        }
    }

    #[allow(dead_code)]
    pub fn move_word_backward(&mut self) {
        if self.text_buffer.is_empty() {
            return;
        }
        let mut line = self.cursor.0.min(self.text_buffer.len() - 1);
        let mut col = self.cursor.1;
        loop {
            let line_str = &self.text_buffer[line];
            let starts = char_starts(line_str);
            let chars: Vec<char> = line_str.chars().collect();
            let mut pos = char_index(&starts, col);

            if pos < chars.len() && chars[pos].is_whitespace() {
                while pos > 0 && chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
            }
            while pos > 0 && !chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            if pos > 0 {
                self.cursor = (line, starts[pos]);
                return;
            }
            if line == 0 {
                self.cursor = (0, 0);
                return;
            }
            line -= 1;
            col = self.text_buffer[line].len();
        }
    }

    #[allow(dead_code)]
    pub fn move_word_end(&mut self) {
        if self.text_buffer.is_empty() {
            return;
        }
        let mut line = self.cursor.0.min(self.text_buffer.len() - 1);
        let mut col = self.cursor.1;
        loop {
            let line_str = &self.text_buffer[line];
            let starts = char_starts(line_str);
            let chars: Vec<char> = line_str.chars().collect();
            let mut pos = char_index(&starts, col);

            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            if pos < chars.len() {
                while pos + 1 < chars.len() && !chars[pos + 1].is_whitespace() {
                    pos += 1;
                }
                self.cursor = (line, starts[pos]);
                return;
            }
            if line + 1 >= self.text_buffer.len() {
                self.cursor = (line, line_str.len());
                return;
            }
            line += 1;
            col = 0;
        }
    }

    pub fn move_to_line_number(&mut self, line_num: usize) {
        let last = self.text_buffer.len().saturating_sub(1);
        let line = line_num.saturating_sub(1).min(last);
        self.cursor = (line, 0);
    }

    fn clamp_column(&self, col: usize) -> usize {
        let (line, _) = self.cursor;
        if line < self.text_buffer.len() {
            floor_char_boundary(&self.text_buffer[line], col)
        } else {
            0
        }
    }

    pub fn ensure_cursor_visible(&mut self, viewport_height: usize) {
        let height = viewport_height.max(1);
        let max_scroll = self.text_buffer.len().saturating_sub(height);
        if self.cursor.0 < self.scroll_offset {
            self.scroll_offset = self.cursor.0;
        } else if self.cursor.0 >= self.scroll_offset + height {
            self.scroll_offset = self.cursor.0 + 1 - height;
        }
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    pub fn search(&mut self, query: &str) {
        self.search_results.clear();
        self.search_query = query.to_string();
        if query.is_empty() {
            self.set_status_error("E486: Empty pattern");
            return;
        }
        for (line_idx, line) in self.text_buffer.iter().enumerate() {
            let mut from = 0;
            while from <= line.len() {
                let Some(rel) = line[from..].find(query) else {
                    break;
                };
                let pos = from + rel;
                self.search_results.push((line_idx, pos));
                from = pos + query.len();
            }
        }
        self.current_search_result = 0;
        if let Some(&(line, col)) = self.search_results.first() {
            self.cursor = (line, col);
            self.set_status(format!(
                "{} {} for '{}'",
                self.search_results.len(),
                if self.search_results.len() == 1 {
                    "match"
                } else {
                    "matches"
                },
                query
            ));
        } else {
            self.set_status_error(format!("E486: Pattern not found: '{}'", query));
        }
    }

    pub fn next_search_result(&mut self) {
        if !self.search_results.is_empty() {
            self.current_search_result =
                (self.current_search_result + 1) % self.search_results.len();
            let (line, col) = self.search_results[self.current_search_result];
            self.cursor = (line, col);
        }
    }

    pub fn prev_search_result(&mut self) {
        if !self.search_results.is_empty() {
            self.current_search_result = (self.current_search_result + self.search_results.len()
                - 1)
                % self.search_results.len();
            let (line, col) = self.search_results[self.current_search_result];
            self.cursor = (line, col);
        }
    }

    #[allow(dead_code)]
    pub fn yank_line(&mut self) {
        let line = self.cursor.0;
        if line < self.text_buffer.len() {
            self.yank_buffer = vec![self.text_buffer[line].clone()];
            self.set_status("1 line yanked");
        }
    }

    pub fn yank_visual(&mut self) {
        let (start_line, end_line) = self.get_visual_range();
        if start_line < self.text_buffer.len() {
            let end_line = end_line.min(self.text_buffer.len() - 1);
            self.yank_buffer = self.text_buffer[start_line..=end_line].to_vec();
            self.set_status(format!("{} lines yanked", end_line - start_line + 1));
        }
    }

    #[allow(dead_code)]
    pub fn paste(&mut self) {
        if self.yank_buffer.is_empty() {
            return;
        }
        let line = self.cursor.0.min(self.text_buffer.len());
        for (i, yanked_line) in self.yank_buffer.iter().enumerate() {
            self.text_buffer.insert(line + i, yanked_line.clone());
        }
        self.cursor = (line, 0);
        self.modified = true;
        self.update_counts();
        self.set_status("Pasted from register");
    }

    #[allow(dead_code)]
    pub fn paste_before(&mut self) {
        if self.yank_buffer.is_empty() {
            return;
        }
        let line = self.cursor.0.saturating_sub(1).min(self.text_buffer.len());
        for (i, yanked_line) in self.yank_buffer.iter().enumerate() {
            self.text_buffer.insert(line + i, yanked_line.clone());
        }
        self.cursor.0 = line;
        self.modified = true;
        self.update_counts();
        self.set_status("Pasted before cursor");
    }

    #[allow(dead_code)]
    pub fn delete_line(&mut self) {
        let line = self.cursor.0;
        if line >= self.text_buffer.len() {
            return;
        }
        self.yank_buffer = vec![self.text_buffer[line].clone()];
        if self.text_buffer.len() == 1 {
            self.text_buffer[0].clear();
        } else {
            self.text_buffer.remove(line);
        }
        self.cursor.0 = self.cursor.0.min(self.text_buffer.len() - 1);
        self.cursor.1 = 0;
        self.modified = true;
        self.update_counts();
        self.set_status("1 line deleted");
    }

    #[allow(dead_code)]
    pub fn change_line(&mut self) {
        let line = self.cursor.0;
        if line >= self.text_buffer.len() {
            return;
        }
        self.yank_buffer = vec![self.text_buffer[line].clone()];
        if self.text_buffer.len() == 1 {
            self.text_buffer[0].clear();
        } else {
            self.text_buffer.remove(line);
        }
        self.cursor.0 = self.cursor.0.min(self.text_buffer.len() - 1);
        self.cursor.1 = 0;
        self.modified = true;
        self.update_counts();
    }

    #[allow(dead_code)]
    pub fn delete_word(&mut self) {
        let (line, col) = self.cursor;
        if line >= self.text_buffer.len() {
            return;
        }
        let line_str = &mut self.text_buffer[line];
        let len = line_str.len();
        let mut start = floor_char_boundary(line_str, col);
        while start < len {
            let ch = line_str[start..]
                .chars()
                .next()
                .expect("start is a char boundary");
            if !ch.is_whitespace() {
                break;
            }
            start += ch.len_utf8();
        }
        let mut end = start;
        while end < len {
            let ch = line_str[end..]
                .chars()
                .next()
                .expect("end is a char boundary");
            if ch.is_whitespace() {
                break;
            }
            end += ch.len_utf8();
        }
        if end <= start {
            while start < len {
                let ch = line_str[start..]
                    .chars()
                    .next()
                    .expect("start is a char boundary");
                if !ch.is_whitespace() {
                    break;
                }
                start += ch.len_utf8();
            }
            end = start;
        }
        let yanked = line_str[start..end].to_string();
        line_str.replace_range(start..end, "");
        self.yank_buffer = vec![yanked];
        self.cursor.1 = start;
        self.modified = true;
        self.update_counts();
    }

    pub fn delete_visual(&mut self) {
        let (start_line, end_line) = self.get_visual_range();
        if start_line >= self.text_buffer.len() {
            return;
        }
        let end_line = end_line.min(self.text_buffer.len() - 1);
        self.yank_buffer = self.text_buffer[start_line..=end_line].to_vec();
        for _ in 0..(end_line - start_line) {
            self.text_buffer.remove(start_line + 1);
        }
        self.text_buffer.remove(start_line);
        if self.text_buffer.is_empty() {
            self.text_buffer.push(String::new());
        }
        self.cursor.0 = start_line.min(self.text_buffer.len() - 1);
        self.cursor.1 = 0;
        self.modified = true;
        self.update_counts();
    }

    #[allow(dead_code)]
    pub fn copy_line(&mut self) {
        let line = self.cursor.0;
        if line < self.text_buffer.len() {
            self.text_buffer
                .insert(line + 1, self.text_buffer[line].clone());
            self.cursor.0 += 1;
            self.modified = true;
            self.update_counts();
        }
    }

    pub fn indent_line(&mut self) {
        let line = self.cursor.0;
        if line < self.text_buffer.len() {
            let indent = " ".repeat(self.tab_size);
            let original_col = self.cursor.1;
            self.text_buffer[line].insert_str(0, &indent);
            self.cursor.1 = indent.len() + original_col;
            self.modified = true;
        }
    }

    pub fn dedent_line(&mut self) {
        let line = self.cursor.0;
        if line < self.text_buffer.len() {
            let line_str = &mut self.text_buffer[line];
            let spaces = line_str.chars().take_while(|ch| *ch == ' ').count();
            let remove = spaces.min(self.tab_size);
            if remove > 0 {
                line_str.drain(0..remove);
                self.cursor.1 = self.cursor.1.saturating_sub(remove);
                self.modified = true;
            }
        }
    }

    pub fn substitute(&mut self, pattern: &str, replacement: &str, global: bool) -> bool {
        let line = self.cursor.0;
        if line >= self.text_buffer.len() {
            return false;
        }
        let before = self.text_buffer[line].clone();
        let after = if global {
            before.replace(pattern, replacement)
        } else {
            before.replacen(pattern, replacement, 1)
        };
        let changed = after != before;
        if changed {
            self.text_buffer[line] = after;
            self.cursor.1 = self.cursor.1.min(self.text_buffer[line].len());
            self.modified = true;
            self.update_counts();
        }
        changed
    }

    pub fn substitute_all(&mut self, pattern: &str, replacement: &str) -> bool {
        let mut changed = false;
        for line_str in &mut self.text_buffer {
            let after = line_str.replace(pattern, replacement);
            if after != *line_str {
                *line_str = after;
                changed = true;
            }
        }
        if changed {
            self.cursor.1 = floor_char_boundary(&self.text_buffer[self.cursor.0], self.cursor.1);
            self.modified = true;
            self.update_counts();
        }
        changed
    }

    pub fn execute_ex_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }

        match cmd {
            ":wq" | ":x" | ":wq!" => {
                self.add_to_history(cmd.to_string());
                if self.save_current_file() {
                    self.running = false;
                }
                return;
            }
            ":q!" => {
                self.add_to_history(cmd.to_string());
                self.running = false;
                return;
            }
            ":q" => {
                self.add_to_history(cmd.to_string());
                if self.modified {
                    self.set_status_error("NoWriteSinceLastChange (add ! to override)");
                } else {
                    self.running = false;
                }
                return;
            }
            ":set" => {
                self.set_status(format!(
                    "autoindent={}, tabstop={}, wrap={}, number={}, showmode={}",
                    self.auto_indent,
                    self.tab_size,
                    self.word_wrap,
                    self.show_line_numbers,
                    self.show_mode_in_status
                ));
                return;
            }
            ":undo" => {
                self.set_status("Undo not yet implemented");
                return;
            }
            ":redo" => {
                self.set_status("Redo not yet implemented");
                return;
            }
            ":help" => {
                self.mode = AppMode::HelpMenu;
                self.set_status("");
                return;
            }
            ":explorer" | ":tree" | ":ex" => {
                self.refresh_explorer();
                self.set_panel(ActivePanel::Explorer);
                return;
            }
            ":terminal" | ":term" => {
                self.set_panel(ActivePanel::Terminal);
                return;
            }
            ":problems" | ":diagnostics" => {
                self.set_panel(ActivePanel::Problems);
                return;
            }
            ":git" => {
                self.refresh_git();
                return;
            }
            ":output" => {
                self.set_panel(ActivePanel::Output);
                return;
            }
            ":history" => {
                self.set_panel(ActivePanel::History);
                return;
            }
            ":build" => {
                self.run_task(TaskKind::Build);
                return;
            }
            ":run" => {
                self.run_task(TaskKind::Run);
                return;
            }
            ":test" => {
                self.run_task(TaskKind::Test);
                return;
            }
            ":check" => {
                self.run_task(TaskKind::Check);
                return;
            }
            ":fmt" => {
                self.run_task(TaskKind::Fmt);
                return;
            }
            ":lsp" => {
                self.start_lsp();
                return;
            }
            ":complete" => {
                self.request_completion();
                return;
            }
            ":definition" | ":def" => {
                self.request_definition();
                return;
            }
            ":hover" => {
                self.request_hover();
                return;
            }
            ":tabs" => {
                let labels: Vec<String> = self
                    .buffers
                    .iter()
                    .enumerate()
                    .map(|(i, buffer)| {
                        format!(
                            "{}:{}",
                            if i == self.active_tab { "*" } else { " " },
                            buffer.label()
                        )
                    })
                    .collect();
                self.set_status(labels.join("  "));
                return;
            }
            ":tabnew" => {
                self.new_tab();
                return;
            }
            ":tabnext" | ":bn" => {
                self.next_tab();
                return;
            }
            ":tabprev" | ":bp" => {
                self.prev_tab();
                return;
            }
            ":tabclose" => {
                self.close_tab();
                return;
            }
            ":tabclose!" => {
                self.close_tab_force();
                return;
            }
            ":saveall" | ":wall" => {
                self.save_all();
                return;
            }
            ":refresh" => {
                self.refresh_explorer();
                self.refresh_git();
                self.set_status("Workspace refreshed");
                return;
            }
            ":print" | ":p" => {
                self.print_text();
                return;
            }
            ":number" | ":nu" => {
                self.show_line_numbers = true;
                self.set_status("Line numbers enabled");
                return;
            }
            ":nonumber" | ":nonu" => {
                self.show_line_numbers = false;
                self.set_status("Line numbers disabled");
                return;
            }
            ":file" | ":f" => {
                let name = self.file_name.as_deref().unwrap_or("No name");
                self.set_status(format!(
                    "{} \"{}\" {}L, {}C",
                    if self.modified { "[+]" } else { "" },
                    name,
                    self.line_count,
                    self.char_count
                ));
                return;
            }
            _ => {}
        }

        if let Some(rest) = cmd.strip_prefix(":w") {
            let rest = rest.trim();
            if rest.is_empty() || rest == "!" {
                self.save_current_file();
            } else {
                self.save_to_file(rest);
            }
            self.add_to_history(cmd.to_string());
            return;
        }

        if cmd == ":e" {
            self.set_status("E345: Cannot edit without a name");
            return;
        }

        let edit_target = cmd
            .strip_prefix(":e!")
            .map(|rest| (true, rest.trim()))
            .or_else(|| cmd.strip_prefix(":e ").map(|rest| (false, rest.trim())));

        if let Some((force, name)) = edit_target {
            if name.is_empty() {
                self.set_status("E345: Cannot edit without a name");
            } else if self.modified && !force {
                self.set_status_error("E37: No write since last change (add ! to override)");
            } else {
                self.open_file(name);
            }
            self.add_to_history(cmd.to_string());
            return;
        }

        if let Some(rest) = cmd.strip_prefix("%s/").or_else(|| cmd.strip_prefix(":%s/")) {
            if let Some((pattern, replacement, _global)) = parse_substitute(rest) {
                let changed = self.substitute_all(pattern, replacement);
                self.set_status(if changed {
                    "Substitution made in entire buffer"
                } else {
                    "No substitution made"
                });
            } else {
                self.set_status_error("E486: Invalid substitute pattern");
            }
            self.add_to_history(cmd.to_string());
            return;
        }

        if let Some(rest) = cmd.strip_prefix(":s/") {
            if let Some((pattern, replacement, global)) = parse_substitute(rest) {
                let changed = self.substitute(pattern, replacement, global);
                self.set_status(if changed {
                    "Substitution made"
                } else {
                    "No substitution made"
                });
            } else {
                self.set_status_error("E486: Invalid substitute pattern");
            }
            self.add_to_history(cmd.to_string());
            return;
        }

        if let Some(rest) = cmd.strip_prefix(":set") {
            self.handle_set_command(rest);
            self.add_to_history(cmd.to_string());
            return;
        }

        if let Some(num_str) = cmd.strip_prefix(':') {
            if let Ok(num) = num_str.parse::<usize>() {
                self.move_to_line_number(num);
                self.add_to_history(cmd.to_string());
                return;
            }
        }

        self.set_status_error(format!("E492: Not an editor command: {}", cmd));
    }

    fn handle_set_command(&mut self, rest: &str) {
        let rest = rest.trim();
        for part in rest.split(',') {
            let part = part.trim();
            if part == "autoindent" || part == "ai" {
                self.auto_indent = true;
                self.set_status("autoindent enabled");
            } else if part == "noautoindent" || part == "noai" {
                self.auto_indent = false;
                self.set_status("autoindent disabled");
            } else if part == "smartindent" || part == "si" {
                self.auto_indent = true;
                self.set_status("smartindent enabled");
            } else if part == "nosmartindent" || part == "nosi" {
                self.auto_indent = false;
                self.set_status("smartindent disabled");
            } else if part == "wrap" || part == "wr" {
                self.word_wrap = true;
                self.set_status("wrap enabled");
            } else if part == "nowrap" || part == "nowr" {
                self.word_wrap = false;
                self.set_status("wrap disabled");
            } else if part == "number" || part == "nu" {
                self.show_line_numbers = true;
                self.set_status("line numbers enabled");
            } else if part == "nonumber" || part == "nonu" {
                self.show_line_numbers = false;
                self.set_status("line numbers disabled");
            } else if part == "showmode" || part == "sm" {
                self.show_mode_in_status = true;
                self.set_status("showmode enabled");
            } else if part == "noshowmode" || part == "nosm" {
                self.show_mode_in_status = false;
                self.set_status("showmode disabled");
            } else if let Some(val) = part
                .strip_prefix("tabstop=")
                .or_else(|| part.strip_prefix("ts="))
            {
                match val.parse::<usize>() {
                    Ok(ts) => {
                        self.tab_size = ts;
                        self.set_status(format!("tabstop={}", ts));
                    }
                    Err(_) => self.set_status_error(format!("E521: Invalid number: {}", val)),
                }
            } else {
                self.set_status_error(format!("E518: Unknown option: {}", part));
            }
        }
    }

    pub fn sync_view_to_buffer(&mut self) {
        if let Some(buffer) = self.buffers.get_mut(self.active_tab) {
            buffer.path = self.file_name.as_ref().map(PathBuf::from);
            buffer.lines = self.text_buffer.clone();
            buffer.cursor = self.cursor;
            buffer.scroll_offset = self.scroll_offset;
            buffer.modified = self.modified;
            buffer.trailing_newline = self.trailing_newline;
        }
    }

    pub fn sync_buffer_to_view(&mut self) {
        let Some(buffer) = self.buffers.get(self.active_tab) else {
            return;
        };
        self.file_name = buffer
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        self.text_buffer = buffer.lines.clone();
        self.cursor = buffer.cursor;
        self.scroll_offset = buffer.scroll_offset;
        self.modified = buffer.modified;
        self.trailing_newline = buffer.trailing_newline;
        self.update_counts();
    }

    pub fn save_current_file(&mut self) -> bool {
        let filename = self
            .file_name
            .clone()
            .unwrap_or_else(|| String::from("untitled.txt"));
        self.save_to_file(&filename)
    }

    pub fn save_to_file(&mut self, filename: &str) -> bool {
        if let Err(e) = self.write_file(filename) {
            self.set_status_error(format!("E303: Unable to open file: {}", e));
            false
        } else {
            self.modified = false;
            self.file_name = Some(filename.to_string());
            self.sync_view_to_buffer();
            self.add_to_history(format!("Wrote {} lines", self.line_count));
            self.set_status(format!(
                "\"{}\" {}L, {}C written",
                filename, self.line_count, self.char_count
            ));
            if let Some(lsp) = &mut self.lsp {
                let _ = lsp.did_save(&PathBuf::from(filename));
            }
            true
        }
    }

    pub fn save_all(&mut self) -> bool {
        let mut count = 0;
        let mut failed = false;
        for index in 0..self.buffers.len() {
            if !self.buffers[index].modified {
                continue;
            }
            let Some(path) = self.buffers[index].path.clone() else {
                continue;
            };
            let mut file = std::fs::File::create(&path);
            if let Ok(file) = &mut file {
                use std::io::Write;
                let result = (|| -> std::io::Result<()> {
                    for (i, line) in self.buffers[index].lines.iter().enumerate() {
                        if i > 0 {
                            writeln!(file)?;
                        }
                        write!(file, "{}", line)?;
                    }
                    if self.buffers[index].trailing_newline {
                        writeln!(file)?;
                    }
                    Ok(())
                })();
                if result.is_ok() {
                    self.buffers[index].modified = false;
                    count += 1;
                } else {
                    failed = true;
                }
            } else {
                failed = true;
            }
        }
        self.sync_buffer_to_view();
        if failed {
            self.set_status_error("E303: One or more buffers could not be written");
        } else {
            self.set_status(format!("Saved {} modified buffer(s)", count));
        }
        !failed
    }

    pub fn open_file(&mut self, filename: &str) {
        let path = PathBuf::from(filename);
        if let Some(index) = self
            .buffers
            .iter()
            .position(|buffer| buffer.path.as_ref() == Some(&path))
        {
            self.sync_view_to_buffer();
            self.active_tab = index;
            self.sync_buffer_to_view();
            self.set_status(format!("Switched to \"{}\"", filename));
            return;
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.sync_view_to_buffer();
                let trailing_newline = content.ends_with('\n');
                let lines: Vec<String> = content.lines().map(String::from).collect();
                let lines = if lines.is_empty() {
                    vec![String::new()]
                } else {
                    lines
                };
                self.buffers
                    .push(EditorBuffer::from_path(&path, lines, trailing_newline));
                self.active_tab = self.buffers.len() - 1;
                self.sync_buffer_to_view();
                self.set_status(format!(
                    "\"{}\" {}L, {}C",
                    filename, self.line_count, self.char_count
                ));
                self.notify_lsp_open();
            }
            Err(e) => {
                self.set_status_error(format!("E345: Can't open file: {}", e));
            }
        }
    }

    fn write_file(&self, filename: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;
        let mut file = File::create(filename)?;
        for (i, line) in self.text_buffer.iter().enumerate() {
            if i > 0 {
                writeln!(file)?;
            }
            write!(file, "{}", line)?;
        }
        if self.trailing_newline {
            writeln!(file)?;
        }
        Ok(())
    }

    pub fn add_to_history(&mut self, command: String) {
        self.command_history.push_back(command);
        if self.command_history.len() > 50 {
            self.command_history.pop_front();
        }
    }

    pub fn print_text(&mut self) {
        let output = self.printer.print_buffer(&self.text_buffer);
        self.print_history.push_back(output);
        if self.print_history.len() > 20 {
            self.print_history.pop_front();
        }
        self.set_status("Text printed to output panel (Tab/Esc to view)");
        self.active_panel = ActivePanel::Output;
    }

    pub fn project_root(&self) -> PathBuf {
        self.explorer.root.clone()
    }

    pub fn buffer_text(&self) -> String {
        let mut text = self.text_buffer.join("\n");
        if self.trailing_newline {
            text.push('\n');
        }
        text
    }

    pub fn set_panel(&mut self, panel: ActivePanel) {
        self.active_panel = panel;
        self.set_status(match panel {
            ActivePanel::Editor => "Editor",
            ActivePanel::Explorer => "File explorer — j/k move, Enter open, Esc editor",
            ActivePanel::Terminal => "Terminal — type to run, Esc returns to editor",
            ActivePanel::Output => "Task output — Esc returns to editor",
            ActivePanel::Problems => "Problems — j/k select, Enter jump, Esc editor",
            ActivePanel::Git => "Git status — Esc returns to editor",
            ActivePanel::History => "Command history — Esc returns to editor",
        });
    }

    pub fn refresh_explorer(&mut self) {
        self.explorer.refresh();
        self.set_status("File explorer refreshed");
    }

    pub fn explorer_open_selected(&mut self) {
        let Some(entry) = self.explorer.selected_entry().cloned() else {
            return;
        };
        if entry.is_dir {
            self.explorer.toggle();
            return;
        }
        let path = entry.path.to_string_lossy().into_owned();
        self.open_file(&path);
        self.active_panel = ActivePanel::Editor;
    }

    pub fn next_tab(&mut self) {
        if self.buffers.len() <= 1 {
            self.set_status("Only one buffer is open");
            return;
        }
        self.sync_view_to_buffer();
        self.active_tab = (self.active_tab + 1) % self.buffers.len();
        self.sync_buffer_to_view();
        self.set_status(format!(
            "Buffer {}/{}",
            self.active_tab + 1,
            self.buffers.len()
        ));
    }

    pub fn prev_tab(&mut self) {
        if self.buffers.len() <= 1 {
            self.set_status("Only one buffer is open");
            return;
        }
        self.sync_view_to_buffer();
        self.active_tab = (self.active_tab + self.buffers.len() - 1) % self.buffers.len();
        self.sync_buffer_to_view();
        self.set_status(format!(
            "Buffer {}/{}",
            self.active_tab + 1,
            self.buffers.len()
        ));
    }

    pub fn new_tab(&mut self) {
        self.sync_view_to_buffer();
        self.buffers.push(EditorBuffer::untitled());
        self.active_tab = self.buffers.len() - 1;
        self.sync_buffer_to_view();
        self.set_status(format!(
            "New buffer {}/{}",
            self.active_tab + 1,
            self.buffers.len()
        ));
    }

    pub fn close_tab(&mut self) {
        if self.buffers.len() <= 1 {
            self.set_status("Cannot close the last buffer");
            return;
        }
        if self.modified {
            self.set_status_error("E37: No write since last change (add ! to override)");
            return;
        }
        self.buffers.remove(self.active_tab);
        self.active_tab = self.active_tab.min(self.buffers.len() - 1);
        self.sync_buffer_to_view();
        self.set_status(format!("Buffer closed ({} left)", self.buffers.len()));
    }

    pub fn close_tab_force(&mut self) {
        if self.buffers.len() <= 1 {
            self.set_status("Cannot close the last buffer");
            return;
        }
        self.buffers.remove(self.active_tab);
        self.active_tab = self.active_tab.min(self.buffers.len() - 1);
        self.sync_buffer_to_view();
        self.set_status("Buffer closed");
    }

    pub fn jump_to(&mut self, path: &std::path::Path, line: usize, column: usize) {
        let name = path.to_string_lossy().into_owned();
        self.open_file(&name);
        self.move_to_line_number(line.max(1));
        let target = self.cursor.0.min(self.text_buffer.len().saturating_sub(1));
        self.cursor = (
            target,
            floor_char_boundary(&self.text_buffer[target], column),
        );
        self.scroll_offset = self.cursor.0.saturating_sub(5);
        self.active_panel = ActivePanel::Editor;
    }

    pub fn run_task(&mut self, kind: TaskKind) {
        let root = self.project_root();
        self.tasks.start(kind, &root);
        self.task_title = format!("{} (running…)", kind.label());
        if !self.task_output.is_empty() && !self.task_output.ends_with('\n') {
            self.task_output.push('\n');
        }
        self.task_output
            .push_str(&format!("$ {} (running…)\n", kind.label()));
        self.set_panel(ActivePanel::Output);
    }

    pub fn poll_async(&mut self) {
        self.terminal.poll();

        for event in self.tasks.poll() {
            self.handle_task_event(event);
        }

        let messages = match &mut self.lsp {
            Some(lsp) => lsp.poll(),
            None => Vec::new(),
        };
        if !messages.is_empty() {
            self.handle_lsp_messages(messages);
        }
    }

    fn handle_task_event(&mut self, event: TaskEvent) {
        let status = if event.success { "ok" } else { "failed" };
        self.task_title = format!("{} — {}", event.kind.label(), status);
        if !self.task_output.is_empty() && !self.task_output.ends_with('\n') {
            self.task_output.push('\n');
        }
        self.task_output
            .push_str(&format!("$ {} — {}\n", event.kind.label(), status));
        self.task_output.push_str(&event.display);
        if !self.task_output.ends_with('\n') {
            self.task_output.push('\n');
        }
        if self.task_output.len() > 40_000 {
            let start = floor_char_boundary(&self.task_output, self.task_output.len() - 40_000);
            self.task_output = self.task_output[start..].to_string();
        }
        if event.kind == TaskKind::Check {
            self.diagnostics = parse_cargo_diagnostics(&event.output, &self.project_root());
            self.set_status(format!(
                "{} problem(s) from cargo check",
                self.diagnostics.len()
            ));
        } else {
            self.set_status(self.task_title.clone());
        }
        if event.kind == TaskKind::Fmt {
            self.refresh_explorer();
        }
    }

    pub fn refresh_git(&mut self) {
        let root = self.project_root();
        let (branch, status) = git::status(&root);
        self.git_branch = branch;
        self.git_status = status;
        self.set_panel(ActivePanel::Git);
    }

    pub fn open_diagnostic(&mut self, index: usize) {
        let Some(diagnostic) = self.diagnostics.get(index).cloned() else {
            return;
        };
        self.jump_to(&diagnostic.path, diagnostic.line, diagnostic.column);
        self.set_status(diagnostic.message);
    }

    pub fn start_lsp(&mut self) {
        if self.lsp.is_some() {
            self.set_status("rust-analyzer is already running");
            return;
        }
        let root = self.project_root();
        match LspClient::start(&root) {
            Ok(client) => {
                self.lsp = Some(client);
                self.lsp_error = None;
                self.notify_lsp_open();
                self.set_status("rust-analyzer started");
            }
            Err(error) => {
                let message = format!("{error} (install with: rustup component add rust-analyzer)");
                self.lsp_error = Some(message.clone());
                self.set_status_error(message);
            }
        }
    }

    pub fn notify_lsp_open(&mut self) {
        let Some(name) = self.file_name.clone() else {
            return;
        };
        if !name.ends_with(".rs") {
            return;
        }
        let path = PathBuf::from(&name);
        let text = self.buffer_text();
        let Some(lsp) = &mut self.lsp else {
            return;
        };
        if let Err(error) = lsp.did_open(&path, &text) {
            self.lsp_error = Some(error);
        }
    }

    pub fn notify_lsp_change(&mut self) {
        let Some(name) = self.file_name.clone() else {
            return;
        };
        if !name.ends_with(".rs") {
            return;
        }
        let path = PathBuf::from(&name);
        let text = self.buffer_text();
        let Some(lsp) = &mut self.lsp else {
            return;
        };
        if let Err(error) = lsp.did_change(&path, &text) {
            self.lsp_error = Some(error);
        }
    }

    fn lsp_position(&self) -> Option<(PathBuf, usize, usize)> {
        let name = self.file_name.clone()?;
        if !name.ends_with(".rs") {
            return None;
        }
        let line_text = self
            .text_buffer
            .get(self.cursor.0)
            .map(String::as_str)
            .unwrap_or("");
        let col = lsp::byte_col_to_utf16(line_text, self.cursor.1);
        Some((PathBuf::from(name), self.cursor.0, col))
    }

    pub fn request_completion(&mut self) {
        let Some((path, line, col)) = self.lsp_position() else {
            self.set_status_error("LSP completion requires a Rust file");
            return;
        };
        let Some(lsp) = &mut self.lsp else {
            self.set_status_error("rust-analyzer is not running (:lsp)");
            return;
        };
        match lsp.request_completion(&path, line, col) {
            Ok(id) => {
                self.pending_lsp = Some(PendingLsp {
                    id,
                    kind: LspRequestKind::Completion,
                });
            }
            Err(error) => self.set_status_error(error),
        }
    }

    pub fn request_definition(&mut self) {
        let Some((path, line, col)) = self.lsp_position() else {
            self.set_status_error("LSP definition requires a Rust file");
            return;
        };
        let Some(lsp) = &mut self.lsp else {
            self.set_status_error("rust-analyzer is not running (:lsp)");
            return;
        };
        match lsp.request_definition(&path, line, col) {
            Ok(id) => {
                self.pending_lsp = Some(PendingLsp {
                    id,
                    kind: LspRequestKind::Definition,
                });
            }
            Err(error) => self.set_status_error(error),
        }
    }

    pub fn request_hover(&mut self) {
        let Some((path, line, col)) = self.lsp_position() else {
            self.set_status_error("LSP hover requires a Rust file");
            return;
        };
        let Some(lsp) = &mut self.lsp else {
            self.set_status_error("rust-analyzer is not running (:lsp)");
            return;
        };
        match lsp.request_hover(&path, line, col) {
            Ok(id) => {
                self.pending_lsp = Some(PendingLsp {
                    id,
                    kind: LspRequestKind::Hover,
                });
            }
            Err(error) => self.set_status_error(error),
        }
    }

    fn handle_lsp_messages(&mut self, messages: Vec<LspMessage>) {
        for message in messages {
            match message {
                LspMessage::ServerRequest { id, method, .. } => {
                    let result = if method == "workspace/configuration" {
                        serde_json::json!([{}])
                    } else {
                        serde_json::Value::Null
                    };
                    if let Some(lsp) = &mut self.lsp {
                        let _ = lsp.respond(id, result);
                    }
                }
                LspMessage::Notification { method, params } => {
                    if method == "textDocument/publishDiagnostics" {
                        self.apply_lsp_diagnostics(&params);
                    }
                }
                LspMessage::Response { id, result, error } => {
                    if let Some(error) = error {
                        self.set_status_error(format!("LSP: {}", error));
                        continue;
                    }
                    let Some(pending) = self.pending_lsp.take() else {
                        continue;
                    };
                    if pending.id != id {
                        self.pending_lsp = Some(pending);
                        continue;
                    }
                    let result = result.unwrap_or(serde_json::Value::Null);
                    match pending.kind {
                        LspRequestKind::Completion => self.apply_completions(&result),
                        LspRequestKind::Definition => self.apply_definition(&result),
                        LspRequestKind::Hover => self.apply_hover(&result),
                    }
                }
            }
        }
    }

    fn apply_completions(&mut self, result: &serde_json::Value) {
        let items: Vec<&serde_json::Value> = match result {
            serde_json::Value::Array(items) => items.iter().collect(),
            serde_json::Value::Object(object) => object
                .get("items")
                .and_then(serde_json::Value::as_array)
                .map(|items| items.iter().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        self.completions = items
            .iter()
            .filter_map(|item| {
                item.get("label")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .take(50)
            .collect();
        self.completion_index = 0;
        self.set_status(format!(
            "{} completion(s) — Tab accepts, Esc dismisses",
            self.completions.len()
        ));
    }

    pub fn accept_completion(&mut self) -> bool {
        if self.completions.is_empty() {
            return false;
        }
        let Some(text) = self.completions.get(self.completion_index).cloned() else {
            return false;
        };
        for ch in text.chars() {
            self.insert_at_cursor(ch);
        }
        self.notify_lsp_change();
        self.completions.clear();
        self.completion_index = 0;
        self.mode = AppMode::Insert;
        true
    }

    fn apply_definition(&mut self, result: &serde_json::Value) {
        let location = match result {
            serde_json::Value::Array(items) => items.first(),
            other => Some(other),
        };
        let Some(location) = location else {
            self.set_status_error("No definition found");
            return;
        };
        let Some(uri) = location.get("uri").and_then(serde_json::Value::as_str) else {
            self.set_status_error("Definition has no file");
            return;
        };
        let Some(path) = lsp::path_from_uri(uri) else {
            self.set_status_error("Definition path is not a local file");
            return;
        };
        let start = location.get("range").and_then(|r| r.get("start"));
        let line = start
            .and_then(|s| s.get("line"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        let character = start
            .and_then(|s| s.get("character"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        self.jump_to(&path, line + 1, character);
        self.set_status(format!("Jumped to {}", path.display()));
    }

    fn apply_hover(&mut self, result: &serde_json::Value) {
        let contents = result.get("contents");
        let text = match contents {
            Some(serde_json::Value::String(text)) => text.clone(),
            Some(serde_json::Value::Object(object)) => object
                .get("value")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string(),
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .filter_map(|item| match item {
                    serde_json::Value::String(text) => Some(text.clone()),
                    serde_json::Value::Object(object) => object
                        .get("value")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        };
        if text.is_empty() {
            self.set_status("No hover information");
            return;
        }
        self.task_title = "Hover".to_string();
        self.task_output = text;
        self.set_panel(ActivePanel::Output);
    }

    fn apply_lsp_diagnostics(&mut self, params: &serde_json::Value) {
        let Some(uri) = params.get("uri").and_then(serde_json::Value::as_str) else {
            return;
        };
        let Some(path) = lsp::path_from_uri(uri) else {
            return;
        };
        let Some(items) = params
            .get("diagnostics")
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };

        self.diagnostics
            .retain(|diagnostic| diagnostic.path != path);
        for item in items {
            let Some(message) = item.get("message").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let start = item.get("range").and_then(|r| r.get("start"));
            let line = start
                .and_then(|s| s.get("line"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize
                + 1;
            let column = start
                .and_then(|s| s.get("character"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize
                + 1;
            let severity = match item.get("severity").and_then(serde_json::Value::as_u64) {
                Some(1) => Severity::Error,
                Some(2) => Severity::Warning,
                _ => Severity::Other,
            };
            self.diagnostics.push(Diagnostic {
                path: path.clone(),
                line,
                column,
                severity,
                message: message.to_string(),
            });
        }
    }

    pub fn get_visual_range(&self) -> (usize, usize) {
        if self.visual_start.0 <= self.cursor.0 {
            (self.visual_start.0, self.cursor.0)
        } else {
            (self.cursor.0, self.visual_start.0)
        }
    }
}

fn parse_cargo_diagnostics(output: &str, root: &std::path::Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for line in output.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("reason").and_then(serde_json::Value::as_str) != Some("compiler-message") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let text = message
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let severity = match message.get("level").and_then(serde_json::Value::as_str) {
            Some("error") => Severity::Error,
            Some("warning") => Severity::Warning,
            _ => Severity::Other,
        };
        let Some(spans) = message.get("spans").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for span in spans {
            let Some(file_name) = span.get("file_name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let path = if std::path::Path::new(file_name).is_absolute() {
                PathBuf::from(file_name)
            } else {
                root.join(file_name)
            };
            diagnostics.push(Diagnostic {
                path: path.clone(),
                line: span
                    .get("line_start")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1) as usize,
                column: span
                    .get("column_start")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1) as usize,
                severity,
                message: text.clone(),
            });
        }
    }
    diagnostics
}

pub fn run() {
    let mut app = App::new();

    if let Err(e) = crossterm::terminal::enable_raw_mode() {
        eprintln!("Failed to enable raw mode: {}", e);
        return;
    }

    let _terminal_guard = TerminalGuard;

    let mut stdout = std::io::stdout();
    if let Err(e) = crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen) {
        eprintln!("Failed to enter alternate screen: {}", e);
        return;
    }
    let _ = crossterm::execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
    );
    let _ = crossterm::execute!(stdout, crossterm::cursor::Hide);
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = match ratatui::Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to create terminal: {}", e);
            return;
        }
    };

    loop {
        app.poll_async();

        let size = terminal
            .size()
            .unwrap_or(ratatui::layout::Size::new(100, 24));
        let viewport_height = (size.height as usize).saturating_sub(1).max(1);
        app.terminal
            .resize(viewport_height as u16, size.width.max(1));
        app.ensure_cursor_visible(viewport_height);

        if let Err(e) = terminal.draw(|frame| ui::render(frame, &app)) {
            eprintln!("Render error: {}", e);
            break;
        }
        if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(100)) {
            if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                if key.kind == crossterm::event::KeyEventKind::Press
                    && event_handler::handle_key(&mut app, key)
                {
                    break;
                }
            }
        }
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> String {
        format!(
            "{}/opencode-rust-{}-{}.txt",
            std::env::temp_dir().display(),
            std::process::id(),
            tag
        )
    }

    #[test]
    fn wq_saves_and_quits() {
        let mut app = App::new();
        app.text_buffer = vec!["hello".to_string()];
        let path = temp_path("wq");
        app.file_name = Some(path.clone());
        app.execute_ex_command(":wq");
        assert!(!app.running);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello\n");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn wq_stays_open_when_save_fails() {
        let mut app = App::new();
        app.text_buffer = vec!["hello".to_string()];
        app.modified = true;
        let path = format!(
            "{}/missing-dir-{}/file.txt",
            std::env::temp_dir().display(),
            std::process::id()
        );
        app.file_name = Some(path);
        app.execute_ex_command(":wq");

        assert!(app.running);
        assert!(app.modified);
        assert!(app.error_message.is_some());
    }

    #[test]
    fn absolute_write_path_keeps_leading_slash() {
        let mut app = App::new();
        app.text_buffer = vec!["data".to_string()];
        let path = temp_path("absolute");
        app.execute_ex_command(&format!(":w {}", path));
        assert!(std::path::Path::new(&path).is_absolute());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "data\n");
        assert!(app.running);
        app.execute_ex_command(":q");
        assert!(!app.running);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn empty_buffer_operations_do_not_panic() {
        let mut app = App::new();
        app.delete_line();
        app.delete_line();
        app.change_line();
        app.move_to_line_number(99);
        app.move_word_forward();
        app.move_word_backward();
        app.move_cursor_right();
        assert_eq!(app.text_buffer, vec![String::new()]);
    }

    #[test]
    fn unicode_editing_uses_character_boundaries() {
        let mut app = App::new();
        app.text_buffer = vec!["é".to_string()];
        app.cursor = (0, 2);
        app.insert_at_cursor('x');
        assert_eq!(app.text_buffer[0], "éx");
        assert_eq!(app.cursor.1, 3);
        app.delete_backward();
        assert_eq!(app.text_buffer[0], "é");
        assert_eq!(app.cursor.1, 2);
        app.move_cursor_left();
        assert_eq!(app.cursor.1, 0);
        app.move_cursor_right();
        assert_eq!(app.cursor.1, 2);
    }

    #[test]
    fn word_movement_returns_byte_offsets() {
        let mut app = App::new();
        app.text_buffer = vec!["héllo wörld".to_string()];
        app.cursor = (0, 0);
        app.move_word_forward();
        assert_eq!(app.cursor.1, "héllo ".len());
        app.move_word_end();

        assert_eq!(app.cursor.1, "héllo wörl".len());
        app.move_word_backward();
        assert_eq!(app.cursor.1, "héllo ".len());
    }

    #[test]
    fn search_uses_character_boundaries() {
        let mut app = App::new();
        app.text_buffer = vec!["éclair é".to_string()];
        app.search("é");
        assert_eq!(app.search_results, vec![(0, 0), (0, 8)]);
    }

    #[test]
    fn global_substitute_strips_flag_from_replacement() {
        let mut app = App::new();
        app.text_buffer = vec!["a b a".to_string()];
        app.execute_ex_command("%s/a/x/g");
        assert_eq!(app.text_buffer[0], "x b x");
        assert!(!app.status_message.contains("newg"));
    }

    #[test]
    fn error_survives_until_next_key() {
        let mut app = App::new();
        app.modified = true;
        app.set_status_error("first error");
        app.clear_error();
        assert!(app.error_message.is_none());
        app.execute_ex_command(":q");
        assert!(app.error_message.is_some());
        assert!(app.running);
    }
}
