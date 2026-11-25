use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const MAX_LINES: usize = 800;

pub struct TerminalPane {
    _master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    rx: Option<Receiver<Vec<u8>>>,
    raw: String,
    pub lines: Vec<String>,
    partial: String,
    pub error: Option<String>,
}

impl Default for TerminalPane {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalPane {
    pub fn new() -> Self {
        let mut pane = Self {
            _master: None,
            writer: None,
            child: None,
            rx: None,
            raw: String::new(),
            lines: Vec::new(),
            partial: String::new(),
            error: None,
        };

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        });

        let pair = match pair {
            Ok(pair) => pair,
            Err(err) => {
                pane.error = Some(format!("failed to open pty: {err}"));
                return pane;
            }
        };

        let mut command = CommandBuilder::new(&shell);
        command.env("TERM", "xterm-256color");
        let child = match pair.slave.spawn_command(command) {
            Ok(child) => child,
            Err(err) => {
                pane.error = Some(format!("failed to start {shell}: {err}"));
                return pane;
            }
        };
        let mut reader = match pair.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(err) => {
                pane.error = Some(format!("failed to clone pty reader: {err}"));
                return pane;
            }
        };
        let writer = match pair.master.take_writer() {
            Ok(writer) => writer,
            Err(err) => {
                pane.error = Some(format!("failed to open pty writer: {err}"));
                return pane;
            }
        };
        drop(pair.slave);

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if tx.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        pane._master = Some(pair.master);
        pane.writer = Some(writer);
        pane.child = Some(child);
        pane.rx = Some(rx);
        pane.lines
            .push(format!("[terminal] {shell} (Esc returns to editor)"));
        pane
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        while let Ok(bytes) = rx.try_recv() {
            self.raw.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(index) = self.raw.find('\n') {
                let line: String = self.raw.drain(..=index).collect();
                self.lines.push(clean_ansi(line.trim_end_matches('\n')));
            }
        }
        self.partial = clean_ansi(&self.raw);
        if self.lines.len() > MAX_LINES {
            let excess = self.lines.len() - MAX_LINES;
            self.lines.drain(..excess);
        }
    }

    pub fn display_lines(&self) -> Vec<String> {
        let mut lines = self.lines.clone();
        if !self.partial.is_empty() {
            lines.push(self.partial.clone());
        }
        lines
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        if let Some(master) = &self._master {
            let _ = master.resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn write_str(&mut self, text: &str) -> bool {
        match &mut self.writer {
            Some(writer) => {
                let result = writer
                    .write_all(text.as_bytes())
                    .and_then(|_| writer.flush());
                if result.is_err() {
                    self.error = Some("terminal write failed".to_string());
                    false
                } else {
                    true
                }
            }
            None => false,
        }
    }

    pub fn send_key(&mut self, key: KeyEvent) -> bool {
        let bytes: Option<Vec<u8>> = match key.code {
            KeyCode::Char(ch) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && ch.is_ascii_alphabetic() {
                    Some(vec![(ch.to_ascii_lowercase() as u8) & 0x1f])
                } else if key
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META)
                {
                    Some(vec![0x1b, ch as u8])
                } else {
                    Some(ch.to_string().into_bytes())
                }
            }
            KeyCode::Enter => Some(vec![b'\r']),
            KeyCode::Backspace => Some(vec![0x7f]),
            KeyCode::Tab => Some(vec![b'\t']),
            KeyCode::Up => Some(b"\x1b[A".to_vec()),
            KeyCode::Down => Some(b"\x1b[B".to_vec()),
            KeyCode::Right => Some(b"\x1b[C".to_vec()),
            KeyCode::Left => Some(b"\x1b[D".to_vec()),
            KeyCode::Home => Some(b"\x1b[H".to_vec()),
            KeyCode::End => Some(b"\x1b[F".to_vec()),
            KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
            KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
            KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
            _ => None,
        };

        match bytes {
            Some(bytes) => self.write_str(&String::from_utf8_lossy(&bytes)),
            None => true,
        }
    }
}

impl Drop for TerminalPane {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
        }
    }
}

fn clean_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            continue;
        }
        if ch != '\x1b' {
            output.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(next) = chars.next() {
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some(_) => {
                chars.next();
            }
            None => break,
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane_with_rx() -> (TerminalPane, mpsc::Sender<Vec<u8>>) {
        let (tx, rx) = mpsc::channel();
        let pane = TerminalPane {
            _master: None,
            writer: None,
            child: None,
            rx: Some(rx),
            raw: String::new(),
            lines: Vec::new(),
            partial: String::new(),
            error: None,
        };
        (pane, tx)
    }

    #[test]
    fn poll_shows_prompt_without_newline() {
        let (mut pane, tx) = pane_with_rx();
        tx.send(b"user@host:~$ ".to_vec()).unwrap();
        pane.poll();

        assert_eq!(pane.display_lines(), vec!["user@host:~$ "]);
        assert!(pane.lines.is_empty());
    }

    #[test]
    fn poll_completes_prompt_line_on_newline() {
        let (mut pane, tx) = pane_with_rx();
        tx.send(b"user@host:~$ ".to_vec()).unwrap();
        tx.send(b"ls\n".to_vec()).unwrap();
        pane.poll();

        assert_eq!(pane.lines, vec!["user@host:~$ ls"]);
        assert_eq!(pane.display_lines(), vec!["user@host:~$ ls"]);
    }
}
