# ferrum

A terminal-based IDE built into a Rust application. It combines a simple,
non-modal editor with a file explorer, tabs, Rust syntax highlighting, cargo tasks,
diagnostics, an embedded terminal, git status, and a rust-analyzer LSP client.

No plain-letter keys are commands: letters are typed as text. Everything is done
with Ctrl shortcuts, navigation keys, and `:` commands.

## Features

- File explorer with expandable directories
- Multiple editor tabs/buffers with save/switch/close commands
- Lightweight Rust syntax highlighting
- Diagnostics markers and a Problems panel
- Concurrent async `cargo build`, `cargo run`, `cargo test`, `cargo check`, and `cargo fmt` tasks with accumulated output
- Human-readable `cargo check` output plus structured diagnostics
- Real PTY terminal with shell input and ANSI stripping
- Git branch and working-tree status
- rust-analyzer integration: completions, go-to-definition, hover, publish diagnostics
- Simple key handling (no single-letter commands), search, and ex commands
- In-app help with `:help`

## Requirements

- Stable Rust toolchain (edition 2021)
- `cargo`, `rustc`
- `rust-analyzer` is optional and only needed for LSP features

Install the analyzer if you want completion/definition/hover support:

```bash
rustup component add rust-analyzer
```

## Build and run

```bash
cargo build --release
./target/release/ferrum
```

Or install it into `~/.cargo/bin` and run it from anywhere:

```bash
cargo install --path . --force
ferrum
```

There is also an `install.sh` helper:

```bash
./install.sh
```

Prebuilt binaries are attached to GitHub Releases when a `v*` tag is pushed or when a Release is published from the web interface.
The `release.yml` workflow builds Linux, Windows, macOS Intel, and macOS Apple Silicon binaries.

GitHub also generates its own `Source code (zip)` and `Source code (tar.gz)` archives. Those are not the Ferrum binaries. Look under **Assets** for `ferrum-*.tar.gz` and `ferrum-*.zip`.

For development:

```bash
cargo run
```

## Checks

```bash
cargo check
cargo clippy --all-targets
cargo fmt --check
cargo test
```

## Keyboard shortcuts

The editor starts in insert mode: just type. Letters are never commands.

| Key | Action |
| --- | --- |
| `Tab` | Cycle Editor / Explorer / Terminal / Output / Problems / Git / History |
| `Ctrl+E` | File explorer |
| `Ctrl+T` | Integrated terminal |
| `Ctrl+P` | Problems panel |
| `Ctrl+G` | Refresh git status |
| `Ctrl+B` | `cargo build` |
| `Ctrl+R` | `cargo run` |
| `Ctrl+H` | Command history |
| `Ctrl+Space` | Request LSP completions (in insert mode) |
| `Ctrl+]` | Go to definition (LSP) |
| `Ctrl+V` | Visual selection (`Ctrl+Y` copy, `Ctrl+D` cut) |
| `Ctrl+Q` | Quit when the buffer is unmodified |
| `Esc` | Leave insert mode / return to editor from a side panel / dismiss completions |
| `Enter`, `Insert` | Enter insert mode from normal mode |
| Arrows, `Home`, `End`, `PageUp`/`PageDown` | Move the cursor |
| `/pattern` | Search; `F3` / `Shift+F3` for next/previous match |
| `:q!` | Force quit |

In the explorer: arrows move, `Enter` opens a file or toggles a directory,
`Left` goes to the parent directory, `Right`/`Space` toggles a directory, and `F5` refreshes.

In the terminal panel: type normally and press `Esc` to return to the editor.

## Commands

| Command | Action |
| --- | --- |
| `:help` | Show the in-app help |
| `:e <file>` / `:e! <file>` | Open a file in a new tab / force open |
| `:w` / `:w <file>` | Save current buffer / save as |
| `:wq` / `:x` | Save and quit (stays open if the write fails) |
| `:saveall` / `:wall` | Save all modified buffers |
| `:tabnew` | New empty buffer |
| `:tabnext` / `:bn` | Next buffer |
| `:tabprev` / `:bp` | Previous buffer |
| `:tabclose` / `:tabclose!` | Close buffer / force close |
| `:tabs` | List open buffers |
| `:tree` / `:explorer` / `:ex` | Open the file explorer |
| `:terminal` / `:term` | Open the terminal |
| `:output` | Open the task output panel |
| `:problems` / `:diagnostics` | Open the Problems panel |
| `:git` | Open the git panel |
| `:history` | Open command history |
| `:build` | Run `cargo build` |
| `:run` | Run `cargo run` |
| `:test` | Run `cargo test` |
| `:check` | Run `cargo check` and refresh diagnostics |
| `:fmt` | Run `cargo fmt` |
| `:lsp` | Start rust-analyzer |
| `:complete` | Request completions |
| `:definition` / `:def` | Go to definition |
| `:hover` | Show hover information in the output panel |
| `:refresh` | Refresh explorer and git status |
| `:set ...` | Toggle editor options (`number`, `wrap`, `tabstop=4`, etc.) |
| `:q` / `:q!` | Quit / force quit |

## Project structure

```text
.
├── .github
│   └── workflows
│       └── release.yml
├── .gitignore
├── Cargo.lock
├── Cargo.toml
├── LICENSE
├── README.md
├── install.sh
└── src
    ├── app
    │   ├── event_handler.rs   # Key handling and panel input routing
    │   ├── git.rs             # git status collection
    │   ├── lsp.rs             # rust-analyzer JSON-RPC client
    │   ├── printer.rs         # Text formatting helpers and tests
    │   ├── syntax.rs          # Lightweight Rust tokenizer/highlighter
    │   ├── tasks.rs           # Async cargo task runner
    │   ├── terminal.rs        # PTY terminal pane
    │   ├── ui.rs              # ratatui rendering
    │   └── workspace.rs       # Explorer and editor buffers
    ├── app.rs                 # App state, commands, diagnostics, LSP wiring
    └── main.rs                # Entry point
```

## License

Released under the MIT License. See [`LICENSE`](LICENSE) for details. Modifications,
forks, and commercial use are permitted as long as the copyright notice is retained.

`target/`, `dist/`, and `.idea/` are ignored by git.
