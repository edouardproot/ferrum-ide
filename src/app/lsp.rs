use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

#[derive(Debug)]
pub enum LspMessage {
    Response {
        id: u64,
        result: Option<Value>,
        error: Option<Value>,
    },
    ServerRequest {
        id: Value,
        method: String,
    },
    Notification {
        method: String,
        params: Value,
    },
}

#[derive(Debug)]
pub struct LspClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<LspMessage>,
    next_id: u64,
    versions: std::collections::HashMap<String, u64>,
}

impl LspClient {
    pub fn start(root: &Path) -> Result<Self, String> {
        let analyzer =
            std::env::var("RUST_ANALYZER").unwrap_or_else(|_| "rust-analyzer".to_string());
        let mut child = Command::new(&analyzer)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("cannot start {analyzer}: {err}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "rust-analyzer stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "rust-analyzer stdout unavailable".to_string())?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut content_length: Option<usize> = None;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => return,
                        Ok(_) => {}
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                        content_length = value.trim().parse::<usize>().ok();
                    }
                }

                let Some(length) = content_length else {
                    continue;
                };
                let mut buffer = vec![0u8; length];
                if reader.read_exact(&mut buffer).is_err() {
                    return;
                }
                let Ok(message) = serde_json::from_slice::<Value>(&buffer) else {
                    continue;
                };

                let parsed = if let Some(id) = message.get("id") {
                    let id_value = id.clone();
                    if let Some(method) = message.get("method").and_then(Value::as_str) {
                        Some(LspMessage::ServerRequest {
                            id: id_value,
                            method: method.to_string(),
                        })
                    } else {
                        Some(LspMessage::Response {
                            id: id.as_u64().unwrap_or(0),
                            result: message.get("result").cloned(),
                            error: message.get("error").cloned(),
                        })
                    }
                } else {
                    message.get("method").and_then(Value::as_str).map(|method| {
                        LspMessage::Notification {
                            method: method.to_string(),
                            params: message.get("params").cloned().unwrap_or(Value::Null),
                        }
                    })
                };

                if let Some(parsed) = parsed {
                    if tx.send(parsed).is_err() {
                        return;
                    }
                }
            }
        });

        let mut client = Self {
            child,
            stdin,
            rx,
            next_id: 1,
            versions: std::collections::HashMap::new(),
        };
        client.initialize(root)?;
        Ok(client)
    }

    fn send(&mut self, message: Value) -> Result<(), String> {
        let payload = serde_json::to_string(&message).map_err(|err| err.to_string())?;
        write!(
            self.stdin,
            "Content-Length: {}\r\n\r\n{}",
            payload.len(),
            payload
        )
        .and_then(|_| self.stdin.flush())
        .map_err(|err| format!("rust-analyzer write failed: {err}"))
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn initialize(&mut self, root: &Path) -> Result<(), String> {
        let root_uri = uri(root);
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "workspace".to_string());
        let id = self.next_request_id();
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "workspaceFolders": [{ "uri": root_uri, "name": name }],
                "capabilities": {
                    "textDocument": {
                        "completion": { "completionItem": { "snippetSupport": false } },
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "definition": {}
                    }
                }
            }
        }))?;
        self.wait_for_initialize(id)?;
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }))?;
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeConfiguration",
            "params": { "settings": {} }
        }))?;
        Ok(())
    }

    fn wait_for_initialize(&mut self, id: u64) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return Err("rust-analyzer exited during startup".to_string());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("rust-analyzer did not respond to initialize".to_string());
            }
            match self.rx.recv_timeout(remaining) {
                Ok(LspMessage::Response {
                    id: response_id,
                    error: None,
                    ..
                }) if response_id == id => return Ok(()),
                Ok(LspMessage::Response {
                    id: response_id,
                    error: Some(error),
                    ..
                }) if response_id == id => {
                    return Err(format!("rust-analyzer initialize failed: {error}"))
                }
                Ok(_) => continue,
                Err(RecvTimeoutError::Timeout) => {
                    return Err("rust-analyzer did not respond to initialize".to_string());
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("rust-analyzer connection closed during startup".to_string());
                }
            }
        }
    }

    pub fn did_open(&mut self, path: &Path, text: &str) -> Result<(), String> {
        let key = uri(path);
        self.versions.insert(key.clone(), 1);
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": key,
                    "languageId": "rust",
                    "version": 1,
                    "text": text
                }
            }
        }))
    }

    pub fn did_change(&mut self, path: &Path, text: &str) -> Result<(), String> {
        let key = uri(path);
        let version = self.versions.entry(key.clone()).or_insert(0);
        *version += 1;
        let version = *version;
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": key, "version": version },
                "contentChanges": [{ "text": text }]
            }
        }))
    }

    pub fn did_save(&mut self, path: &Path) -> Result<(), String> {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didSave",
            "params": { "textDocument": { "uri": uri(path) } }
        }))
    }

    pub fn request_completion(
        &mut self,
        path: &Path,
        line: usize,
        col: usize,
    ) -> Result<u64, String> {
        let id = self.next_request_id();
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri(path) },
                "position": { "line": line, "character": col },
                "context": { "triggerKind": 1 }
            }
        }))?;
        Ok(id)
    }

    pub fn request_definition(
        &mut self,
        path: &Path,
        line: usize,
        col: usize,
    ) -> Result<u64, String> {
        let id = self.next_request_id();
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri(path) },
                "position": { "line": line, "character": col }
            }
        }))?;
        Ok(id)
    }

    pub fn request_hover(&mut self, path: &Path, line: usize, col: usize) -> Result<u64, String> {
        let id = self.next_request_id();
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri(path) },
                "position": { "line": line, "character": col }
            }
        }))?;
        Ok(id)
    }

    pub fn respond(&mut self, id: Value, result: Value) -> Result<(), String> {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    pub fn poll(&mut self) -> Vec<LspMessage> {
        let mut messages = Vec::new();
        while let Ok(message) = self.rx.try_recv() {
            messages.push(message);
        }
        messages
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.send(json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": {}
        }));
        let _ = self.child.kill();
    }
}

pub fn uri(path: &Path) -> String {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.display()))
}

pub fn path_from_uri(value: &str) -> Option<PathBuf> {
    Url::parse(value)
        .ok()
        .and_then(|url| url.to_file_path().ok())
}

pub fn byte_col_to_utf16(line: &str, byte_col: usize) -> usize {
    let col = byte_col.min(line.len());
    let mut boundary = 0;
    while boundary < col && !line.is_char_boundary(boundary) {
        boundary += 1;
    }
    line[..boundary].encode_utf16().count()
}
