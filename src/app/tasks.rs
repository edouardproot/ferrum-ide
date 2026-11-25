use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Build,
    Run,
    Test,
    Check,
    Fmt,
}

impl TaskKind {
    pub fn label(self) -> &'static str {
        match self {
            TaskKind::Build => "cargo build",
            TaskKind::Run => "cargo run",
            TaskKind::Test => "cargo test",
            TaskKind::Check => "cargo check",
            TaskKind::Fmt => "cargo fmt",
        }
    }
}

#[derive(Debug)]
pub struct TaskEvent {
    pub kind: TaskKind,

    pub output: String,

    pub display: String,
    pub success: bool,
}

pub struct TaskRunner {
    tx: Sender<TaskEvent>,
    rx: Receiver<TaskEvent>,
    active: usize,
    pub running: bool,
    pub current: Option<TaskKind>,
}

impl TaskRunner {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            active: 0,
            running: false,
            current: None,
        }
    }

    pub fn start(&mut self, kind: TaskKind, cwd: &Path) {
        let tx = self.tx.clone();
        self.active += 1;
        self.running = true;
        self.current = Some(kind);

        let cwd = cwd.to_path_buf();
        thread::spawn(move || {
            let mut command = Command::new("cargo");
            command.current_dir(&cwd);
            match kind {
                TaskKind::Build => {
                    command.arg("build");
                }
                TaskKind::Run => {
                    command.arg("run");
                }
                TaskKind::Test => {
                    command.arg("test");
                }
                TaskKind::Check => {
                    command.args(["check", "--message-format=json"]);
                }
                TaskKind::Fmt => {
                    command.arg("fmt");
                }
            }

            let output = command.output();
            let (text, success) = match output {
                Ok(output) => {
                    let mut text = String::new();
                    text.push_str(&String::from_utf8_lossy(&output.stdout));
                    text.push_str(&String::from_utf8_lossy(&output.stderr));
                    (text, output.status.success())
                }
                Err(err) => (format!("failed to start cargo: {err}"), false),
            };
            let display = if kind == TaskKind::Check {
                readable_check_output(&text)
            } else {
                text.clone()
            };
            let _ = tx.send(TaskEvent {
                kind,
                output: text,
                display,
                success,
            });
        });
    }

    pub fn poll(&mut self) -> Vec<TaskEvent> {
        let mut events = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(event) => {
                    self.active = self.active.saturating_sub(1);
                    events.push(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.active = 0;
                    break;
                }
            }
        }
        self.running = self.active > 0;
        if !self.running {
            self.current = None;
        }
        events
    }
}

fn readable_check_output(raw: &str) -> String {
    let mut messages = Vec::new();
    let mut other = Vec::new();
    for line in raw.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            if !line.trim().is_empty() {
                other.push(line.to_string());
            }
            continue;
        };
        if value.get("reason").and_then(serde_json::Value::as_str) != Some("compiler-message") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let text = message
            .get("rendered")
            .and_then(serde_json::Value::as_str)
            .or_else(|| message.get("message").and_then(serde_json::Value::as_str))
            .unwrap_or("compiler message")
            .to_string();
        messages.push(text);
    }

    let mut result = messages.join("\n");
    if !other.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&other.join("\n"));
    }
    if result.is_empty() {
        result.push_str("cargo check finished with no messages");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: TaskKind) -> TaskEvent {
        TaskEvent {
            kind,
            output: String::new(),
            display: kind.label().to_string(),
            success: true,
        }
    }

    #[test]
    fn poll_returns_results_from_every_running_task() {
        let (tx, rx) = mpsc::channel();
        let mut runner = TaskRunner {
            tx: tx.clone(),
            rx,
            active: 2,
            running: true,
            current: Some(TaskKind::Build),
        };
        tx.send(event(TaskKind::Build)).unwrap();
        tx.send(event(TaskKind::Check)).unwrap();

        let events = runner.poll();
        let kinds: Vec<TaskKind> = events.iter().map(|event| event.kind).collect();
        assert_eq!(kinds, vec![TaskKind::Build, TaskKind::Check]);
        assert!(!runner.running);
        assert_eq!(runner.current, None);
    }

    #[test]
    fn poll_stays_running_until_all_tasks_finish() {
        let (tx, rx) = mpsc::channel();
        let mut runner = TaskRunner {
            tx: tx.clone(),
            rx,
            active: 2,
            running: true,
            current: Some(TaskKind::Build),
        };
        tx.send(event(TaskKind::Build)).unwrap();

        let events = runner.poll();
        assert_eq!(events.len(), 1);
        assert!(runner.running);
        assert_eq!(runner.current, Some(TaskKind::Build));
    }
}
