use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::{syntax, ActivePanel, App, AppMode, Severity};

pub fn render(frame: &mut Frame, app: &App) {
    if app.mode == AppMode::HelpMenu {
        render_help(frame, frame.area());
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(0)])
        .split(chunks[0]);

    render_explorer(
        frame,
        columns[0],
        app,
        app.active_panel == ActivePanel::Explorer,
    );

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(columns[1]);

    render_tabbar(frame, right[0], app);
    let content = right[1];

    match app.active_panel {
        ActivePanel::Editor => render_editor(frame, content, app),
        ActivePanel::Explorer => {
            render_explorer(frame, content, app, true);
        }
        ActivePanel::Terminal => render_terminal(frame, content, app),
        ActivePanel::Output => render_task_output(frame, content, app),
        ActivePanel::Problems => render_problems(frame, content, app),
        ActivePanel::Git => render_git(frame, content, app),
        ActivePanel::History => render_panel(frame, content, app),
    }

    render_status_line(frame, chunks[1], app);

    if app.mode == AppMode::ExCommand || app.mode == AppMode::Search {
        render_command_line(frame, frame.area(), app);
    }
    if !app.completions.is_empty() && app.mode == AppMode::Insert {
        render_completions(frame, content, app);
    }
}

fn visible_range(app: &App, height: usize) -> (usize, usize) {
    let start = app.scroll_offset.min(app.text_buffer.len());
    let end = start
        .saturating_add(height.max(1))
        .min(app.text_buffer.len());
    (start, end)
}

fn render_explorer(frame: &mut Frame, area: ratatui::layout::Rect, app: &App, focused: bool) {
    let lines: Vec<Line> = app
        .explorer
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let indent = " ".repeat(entry.depth.saturating_mul(2));
            let icon = if entry.is_dir {
                if entry.expanded {
                    "▾"
                } else {
                    "▸"
                }
            } else {
                "·"
            };
            let name = entry
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.path.to_string_lossy().into_owned());
            let label = format!("{}{} {}", indent, icon, name);
            let style = if focused && index == app.explorer.selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(50, 50, 150))
                    .add_modifier(Modifier::REVERSED)
            } else if entry.is_dir {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(Span::styled(label, style))
        })
        .collect();

    let border = if focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border))
                .title(" Explorer "),
        )
        .wrap(Wrap { trim: false })
        .scroll((
            app.explorer
                .selected
                .saturating_sub(area.height.saturating_sub(3) as usize / 2) as u16,
            0,
        ));
    frame.render_widget(paragraph, area);
}

fn render_tabbar(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let mut spans = Vec::new();
    for (index, buffer) in app.buffers.iter().enumerate() {
        let active = index == app.active_tab;
        let marker = if buffer.modified { "+" } else { " " };
        let label = format!(" {}{} ", buffer.label(), marker);
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::DarkGray)
                .bg(Color::Rgb(35, 35, 35))
        };
        spans.push(Span::styled(label, style));
    }
    let lsp = match (&app.lsp, &app.lsp_error) {
        (Some(_), _) => " [LSP on]",
        (None, Some(_)) => " [LSP error]",
        (None, None) => "",
    };
    spans.push(Span::styled(
        format!("{}  {}/{} tabs", lsp, app.active_tab + 1, app.buffers.len()),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default()),
        area,
    );
}

fn diagnostic_marker(app: &App, line_index: usize) -> Option<Severity> {
    let name = app.file_name.as_deref()?;
    app.diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.line == line_index + 1 && diagnostic.path.to_string_lossy() == name
        })
        .map(|diagnostic| diagnostic.severity)
}

fn render_editor(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let (text_area, has_line_numbers) = if app.show_line_numbers {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(6), Constraint::Min(0)])
            .split(area);

        let (start, end) = visible_range(app, chunks[1].height as usize);
        let (visual_start, visual_end) = if app.mode == AppMode::Visual {
            app.get_visual_range()
        } else {
            (0, usize::MAX)
        };

        let line_numbers: Vec<Line> = app
            .text_buffer
            .iter()
            .enumerate()
            .skip(start)
            .take(end - start)
            .map(|(i, _)| {
                let is_cursor = i == app.cursor.0;
                let is_visual = i >= visual_start && i <= visual_end && app.mode == AppMode::Visual;
                let marker = diagnostic_marker(app, i);
                let (fg, bg, modifier) = if is_visual {
                    (Color::White, Color::Rgb(50, 50, 150), Modifier::REVERSED)
                } else if is_cursor {
                    (Color::Yellow, Color::Reset, Modifier::BOLD)
                } else {
                    let fg = match marker {
                        Some(Severity::Error) => Color::Red,
                        Some(Severity::Warning) => Color::Yellow,
                        _ => Color::DarkGray,
                    };
                    (fg, Color::Reset, Modifier::empty())
                };
                Line::from(Span::styled(
                    format!("{:>4} │", i + 1),
                    Style::default().fg(fg).bg(bg).add_modifier(modifier),
                ))
            })
            .collect();

        frame.render_widget(
            Paragraph::new(line_numbers).block(Block::default()),
            chunks[0],
        );
        (chunks[1], true)
    } else {
        (area, false)
    };

    let (start, end) = visible_range(app, text_area.height as usize);
    let (visual_start, visual_end) = if app.mode == AppMode::Visual {
        app.get_visual_range()
    } else {
        (0, usize::MAX)
    };

    let text_lines: Vec<Line> = app
        .text_buffer
        .iter()
        .enumerate()
        .skip(start)
        .take(end - start)
        .map(|(i, line)| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            if let Some(severity) = diagnostic_marker(app, i) {
                let color = match severity {
                    Severity::Error => Color::Red,
                    Severity::Warning => Color::Yellow,
                    Severity::Other => Color::Gray,
                };
                spans.push(Span::styled("● ", Style::default().fg(color)));
            }

            let is_cursor_line = i == app.cursor.0;
            let is_visual_line =
                i >= visual_start && i <= visual_end && app.mode == AppMode::Visual;

            if is_cursor_line && app.mode != AppMode::Visual {
                let col = super::floor_char_boundary(line, app.cursor.1);
                spans.extend(syntax::highlight(&line[..col]));
                match line[col..].chars().next() {
                    Some(ch) => {
                        spans.push(Span::styled(
                            ch.to_string(),
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::White)
                                .add_modifier(Modifier::REVERSED),
                        ));
                        let after_col = col + ch.len_utf8();
                        spans.extend(syntax::highlight(&line[after_col..]));
                    }
                    None => spans.push(Span::styled(
                        "▌",
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::REVERSED),
                    )),
                }
            } else if is_visual_line {
                let (start_col, end_col) = if i == visual_start && i == visual_end {
                    (
                        app.visual_start.1.min(app.cursor.1),
                        app.visual_start.1.max(app.cursor.1),
                    )
                } else if i == visual_start {
                    (app.visual_start.1, line.len())
                } else if i == visual_end {
                    (0, app.cursor.1)
                } else {
                    (0, line.len())
                };
                let start_col = super::floor_char_boundary(line, start_col);
                let end_col = super::floor_char_boundary(line, end_col);
                spans.extend(syntax::highlight(&line[..start_col]));
                spans.push(Span::styled(
                    line[start_col..end_col].to_string(),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(50, 50, 150))
                        .add_modifier(Modifier::REVERSED),
                ));
                spans.extend(syntax::highlight(&line[end_col..]));
            } else {
                spans.extend(syntax::highlight(line.as_str()));
            }

            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(text_lines).block(Block::default());
    let paragraph = if app.word_wrap {
        paragraph.wrap(Wrap { trim: false })
    } else {
        paragraph
    };
    frame.render_widget(paragraph, text_area);

    if !has_line_numbers {
        frame.render_widget(Block::default().borders(Borders::LEFT), text_area);
    }
}

fn render_terminal(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let visible = app.terminal.display_lines();
    let mut lines: Vec<Line> = visible
        .iter()
        .map(|line| Line::from(line.as_str()))
        .collect();
    if let Some(error) = &app.terminal.error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from("Terminal is empty"));
    }
    let height = area.height.saturating_sub(2) as usize;
    let offset = lines.len().saturating_sub(height) as u16;
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Terminal "),
        )
        .scroll((offset, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_task_output(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let lines: Vec<Line> = app.task_output.lines().map(Line::from).collect();
    let height = area.height.saturating_sub(2) as usize;
    let offset = lines.len().saturating_sub(height) as u16;
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" {} ", app.task_title)),
        )
        .scroll((offset, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_problems(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let lines: Vec<Line> = if app.diagnostics.is_empty() {
        vec![Line::from("No problems. Use :check or start :lsp.")]
    } else {
        app.diagnostics
            .iter()
            .enumerate()
            .map(|(index, diagnostic)| {
                let (icon, color) = match diagnostic.severity {
                    Severity::Error => ("✗", Color::Red),
                    Severity::Warning => ("⚠", Color::Yellow),
                    Severity::Other => ("•", Color::Gray),
                };
                let style = if index == app.problem_index {
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(50, 50, 150))
                } else {
                    Style::default().fg(color)
                };
                let file = diagnostic
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Line::from(Span::styled(
                    format!(
                        "{} {}:{}:{} — {}",
                        icon, file, diagnostic.line, diagnostic.column, diagnostic.message
                    ),
                    style,
                ))
            })
            .collect()
    };
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" Problems ({}) ", app.diagnostics.len())),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_git(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let mut lines = vec![Line::from(Span::styled(
        format!("branch: {}", app.git_branch),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ))];
    if app.git_status.is_empty() {
        lines.push(Line::from("working tree clean"));
    } else {
        lines.extend(app.git_status.iter().map(|line| {
            let color = if line.starts_with("??") {
                Color::DarkGray
            } else if line.contains('M') {
                Color::Yellow
            } else {
                Color::Green
            };
            Line::from(Span::styled(line.clone(), Style::default().fg(color)))
        }));
    }
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Git "),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_panel(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let (title, lines) = match app.active_panel {
        ActivePanel::Output => (
            " Output — Tab to cycle, Esc for editor ",
            app.task_output.lines().map(Line::from).collect::<Vec<_>>(),
        ),
        ActivePanel::History => (
            " Command history — Tab to cycle, Esc for editor ",
            app.command_history
                .iter()
                .rev()
                .map(|cmd| Line::from(cmd.as_str()))
                .collect::<Vec<_>>(),
        ),
        _ => (" Panel ", Vec::new()),
    };

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_status_line(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let mode_str = match app.mode {
        AppMode::Normal => "NORMAL",
        AppMode::Insert => "INSERT",
        AppMode::Visual => "VISUAL",
        AppMode::Search => "SEARCH",
        AppMode::ExCommand => "COMMAND",
        AppMode::HelpMenu => "HELP",
    };

    let mode_color = match app.mode {
        AppMode::Normal => Color::Green,
        AppMode::Insert => Color::Yellow,
        AppMode::Visual => Color::Magenta,
        AppMode::Search => Color::Cyan,
        AppMode::ExCommand => Color::Blue,
        AppMode::HelpMenu => Color::White,
    };

    let file_info = app.file_name.as_deref().unwrap_or("[No Name]");
    let modified_str = if app.modified { "[+]" } else { "" };
    let panel_str = match app.active_panel {
        ActivePanel::Editor => "",
        ActivePanel::Explorer => " [EXPLORER]",
        ActivePanel::Terminal => " [TERMINAL]",
        ActivePanel::Output => " [OUTPUT]",
        ActivePanel::Problems => " [PROBLEMS]",
        ActivePanel::Git => " [GIT]",
        ActivePanel::History => " [HISTORY]",
    };

    let cursor_line = app.cursor.0 + 1;
    let cursor_col = app.cursor.1 + 1;
    let total_lines = app.line_count;
    let percentage = (cursor_line.saturating_mul(100) / total_lines.max(1)).min(100);
    let position_str = if cursor_line == total_lines && total_lines > 0 {
        "Bot".to_string()
    } else if cursor_line == 1 {
        "Top".to_string()
    } else {
        format!("{}%", percentage)
    };

    let mut status_parts = Vec::new();
    if app.show_mode_in_status {
        status_parts.push(Span::styled(
            format!(" {} ", mode_str),
            Style::default()
                .fg(Color::Black)
                .bg(mode_color)
                .add_modifier(Modifier::BOLD),
        ));
    }
    status_parts.extend([
        Span::styled(
            format!(" {}{}{} ", file_info, modified_str, panel_str),
            Style::default().fg(Color::Gray).bg(Color::Rgb(40, 40, 40)),
        ),
        Span::styled(
            format!(" {} ", position_str),
            Style::default().fg(Color::Gray).bg(Color::Rgb(40, 40, 40)),
        ),
        Span::styled(
            format!(" {}:{} ", cursor_line, cursor_col),
            Style::default().fg(Color::Gray).bg(Color::Rgb(40, 40, 40)),
        ),
    ]);

    if app.tasks.running {
        status_parts.push(Span::styled(
            " busy ",
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(40, 40, 40)),
        ));
    }

    if let Some(ref err) = app.error_message {
        status_parts.push(Span::styled(
            format!(" {} ", err),
            Style::default().fg(Color::Red).bg(Color::Rgb(40, 40, 40)),
        ));
    } else if !app.status_message.is_empty() {
        status_parts.push(Span::styled(
            format!(" {} ", app.status_message),
            Style::default().fg(Color::Gray).bg(Color::Rgb(40, 40, 40)),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(status_parts))
            .style(Style::default().fg(Color::Gray).bg(Color::Rgb(40, 40, 40))),
        area,
    );
}

fn render_command_line(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let command_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let prompt = if app.mode == AppMode::Search { "/" } else { "" };
    let text = if app.mode == AppMode::Search {
        &app.search_query
    } else {
        &app.ex_command
    };
    let command_text = format!("{}{} ", prompt, text);
    frame.render_widget(
        Paragraph::new(Span::raw(command_text))
            .style(Style::default().fg(Color::White).bg(Color::Rgb(40, 40, 40))),
        command_area,
    );
}

fn render_completions(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let height = app.completions.len().min(10) as u16 + 2;
    let width = area.width.min(50);
    let x = area.x;
    let y = area.y + area.height.saturating_sub(height + 1);
    let popup = ratatui::layout::Rect {
        x,
        y,
        width,
        height,
    };
    let lines: Vec<Line> = app
        .completions
        .iter()
        .enumerate()
        .take(10)
        .map(|(index, item)| {
            let style = if index == app.completion_index {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(item.clone(), style))
        })
        .collect();
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Completions "),
        )
        .style(Style::default().bg(Color::Rgb(30, 30, 30)));
    frame.render_widget(paragraph, popup);
}

fn render_help(frame: &mut Frame, area: ratatui::layout::Rect) {
    let window_width = 78;
    let window_height = 38;
    let x = (area.width.saturating_sub(window_width)) / 2;
    let y = (area.height.saturating_sub(window_height)) / 2;
    let window_area = ratatui::layout::Rect {
        x,
        y,
        width: window_width.min(area.width),
        height: window_height.min(area.height),
    };

    frame.render_widget(
        Paragraph::new("").style(Style::default().bg(Color::Rgb(0, 0, 0))),
        area,
    );

    let help_text = vec![
        Line::from(Span::styled(
            "  Rust IDE (TUI) — Help",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("  PANELS:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("    Tab           — Cycle editor / explorer / terminal / output / problems / git / history"),
        Line::from("    Ctrl+E         — File explorer (arrows, Enter, Left/Right, Space, F5)"),
        Line::from("    Ctrl+T         — Integrated terminal (Esc returns)"),
        Line::from("    Ctrl+P         — Problems panel"),
        Line::from("    Ctrl+G         — Refresh git status"),
        Line::from(""),
        Line::from(Span::styled("  BUILD & CHECK:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("    Ctrl+B / :build — cargo build"),
        Line::from("    Ctrl+R / :run   — cargo run"),
        Line::from("    :test / :check  — cargo test / cargo check (diagnostics)"),
        Line::from("    :fmt            — cargo fmt"),
        Line::from(""),
        Line::from(Span::styled("  RUST-ANALYZER:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("    :lsp           — Start rust-analyzer"),
        Line::from("    Ctrl+Space     — Request completions (Tab accepts, Esc dismisses)"),
        Line::from("    Ctrl+] / :def  — Go to definition"),
        Line::from("    :hover         — Hover information in output panel"),
        Line::from(""),
        Line::from(Span::styled("  BUFFERS & FILES:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("    :e {file}      — Open file in a new tab"),
        Line::from("    :tabnew        — New empty buffer"),
        Line::from("    :tabnext/:tabprev — Switch buffers"),
        Line::from("    :tabclose      — Close current buffer"),
        Line::from("    :saveall       — Write all modified buffers"),
        Line::from(""),
        Line::from(Span::styled("  EDITOR:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("    No plain-letter keys are commands; type directly."),
        Line::from("    Arrows/Home/End— Move cursor"),
        Line::from("    Enter / Insert — Start typing (Esc returns to normal mode)"),
        Line::from("    Ctrl+V         — Visual selection; Ctrl+Y copy, Ctrl+D cut"),
        Line::from("    /pattern       — Search; F3 / Shift+F3 next/previous"),
        Line::from("    :w/:wq/:q      — Save / save+quit / quit (no quit if write fails)"),
        Line::from("    :help          — This help"),
        Line::from(""),
        Line::from("    Ctrl+Q quits (Esc returns to the editor)."),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Help "),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, window_area);
}
