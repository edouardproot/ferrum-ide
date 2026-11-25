use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::tasks::TaskKind;
use super::{ActivePanel, App, AppMode};

fn is_text_key(key: &KeyEvent) -> bool {
    !key.modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META,
    )
}

fn request_quit(app: &mut App) -> bool {
    if app.modified {
        app.set_status_error("NoWriteSinceLastChange (save first or use :q!)");
        false
    } else {
        true
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    app.clear_error();
    app.sync_view_to_buffer();

    if key.code == KeyCode::Char('q')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META)
        && app.active_panel != ActivePanel::Terminal
    {
        return request_quit(app);
    }

    let modified_before = app.modified;
    let mut should_quit = false;
    match app.mode {
        AppMode::Normal => should_quit = handle_normal_mode(app, key),
        AppMode::Insert => handle_insert_mode(app, key),
        AppMode::Visual => handle_visual_mode(app, key),
        AppMode::Search => handle_search_mode(app, key),
        AppMode::ExCommand => should_quit = handle_ex_mode(app, key),
        AppMode::HelpMenu => handle_help_menu(app, key),
    }

    app.sync_view_to_buffer();
    let edited = app.modified != modified_before
        || app.mode == AppMode::Insert
        || matches!(
            key.code,
            KeyCode::Backspace | KeyCode::Delete | KeyCode::Enter | KeyCode::Tab
        );
    if edited && app.lsp.is_some() {
        app.notify_lsp_change();
    }
    should_quit
}

fn cycle_panel(app: &mut App) {
    app.active_panel = match app.active_panel {
        ActivePanel::Editor => ActivePanel::Explorer,
        ActivePanel::Explorer => ActivePanel::Terminal,
        ActivePanel::Terminal => ActivePanel::Output,
        ActivePanel::Output => ActivePanel::Problems,
        ActivePanel::Problems => ActivePanel::Git,
        ActivePanel::Git => ActivePanel::History,
        ActivePanel::History => ActivePanel::Editor,
    };
    let label = match app.active_panel {
        ActivePanel::Editor => "Editor",
        ActivePanel::Explorer => "File explorer",
        ActivePanel::Terminal => "Terminal",
        ActivePanel::Output => "Task output",
        ActivePanel::Problems => "Problems",
        ActivePanel::Git => "Git status",
        ActivePanel::History => "Command history",
    };
    app.set_status(label);
}

fn handle_panel_key(app: &mut App, key: KeyEvent) {
    match app.active_panel {
        ActivePanel::Explorer => match key.code {
            KeyCode::Down => app.explorer.move_selection(1),
            KeyCode::Up => app.explorer.move_selection(-1),
            KeyCode::Enter => app.explorer_open_selected(),
            KeyCode::Left => app.explorer.parent(),
            KeyCode::Right | KeyCode::Char(' ') => app.explorer.toggle(),
            KeyCode::F(5) => app.refresh_explorer(),
            KeyCode::Esc => app.set_panel(ActivePanel::Editor),
            _ => {}
        },
        ActivePanel::Terminal => {
            if key.code == KeyCode::Esc
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                app.set_panel(ActivePanel::Editor);
            } else {
                app.terminal.send_key(key);
            }
        }
        ActivePanel::Problems => match key.code {
            KeyCode::Down => {
                if !app.diagnostics.is_empty() {
                    app.problem_index = (app.problem_index + 1) % app.diagnostics.len();
                }
            }
            KeyCode::Up => {
                if !app.diagnostics.is_empty() {
                    app.problem_index =
                        (app.problem_index + app.diagnostics.len() - 1) % app.diagnostics.len();
                }
            }
            KeyCode::Enter => app.open_diagnostic(app.problem_index),
            KeyCode::Esc => app.set_panel(ActivePanel::Editor),
            _ => {}
        },
        ActivePanel::Git | ActivePanel::Output | ActivePanel::History => {
            if key.code == KeyCode::Esc {
                app.set_panel(ActivePanel::Editor);
            }
        }
        ActivePanel::Editor => {}
    }
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) -> bool {
    if app.active_panel != ActivePanel::Editor {
        handle_panel_key(app, key);
        return false;
    }

    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        match key.code {
            KeyCode::Char('e') => app.set_panel(ActivePanel::Explorer),
            KeyCode::Char('t') => app.set_panel(ActivePanel::Terminal),
            KeyCode::Char('p') => app.set_panel(ActivePanel::Problems),
            KeyCode::Char('g') => app.refresh_git(),
            KeyCode::Char('b') => app.run_task(TaskKind::Build),
            KeyCode::Char('r') => app.run_task(TaskKind::Run),
            KeyCode::Char(' ') => app.request_completion(),
            KeyCode::Char(']') => app.request_definition(),
            KeyCode::Char('h') => app.set_panel(ActivePanel::History),
            KeyCode::Char('u') => app.set_status("Undo not yet implemented"),
            KeyCode::Char('v') => {
                app.mode = AppMode::Visual;
                app.visual_start = app.cursor;
            }
            _ => {}
        }
        return false;
    }

    match key.code {
        KeyCode::Esc => app.set_panel(ActivePanel::Editor),
        KeyCode::Tab => cycle_panel(app),

        KeyCode::Enter | KeyCode::Insert => app.mode = AppMode::Insert,

        KeyCode::Char('/') => {
            app.mode = AppMode::Search;
            app.search_query.clear();
        }
        KeyCode::Char(':') => {
            app.mode = AppMode::ExCommand;
            app.ex_command = ":".to_string();
        }

        KeyCode::F(3) => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.prev_search_result();
            } else {
                app.next_search_result();
            }
        }

        KeyCode::Delete => app.delete_at_cursor(),
        KeyCode::Backspace => app.delete_backward(),

        KeyCode::Char('0') | KeyCode::Home => app.move_cursor_to_line_start(),
        KeyCode::Char('$') | KeyCode::End => app.move_cursor_to_line_end(),
        KeyCode::Char('^') => app.move_to_first_non_blank(),

        KeyCode::Up => app.move_cursor_up(),
        KeyCode::Down => app.move_cursor_down(),
        KeyCode::Left => app.move_cursor_left(),
        KeyCode::Right => app.move_cursor_right(),

        KeyCode::PageUp => {
            for _ in 0..10 {
                app.move_cursor_up();
            }
        }
        KeyCode::PageDown => {
            for _ in 0..10 {
                app.move_cursor_down();
            }
        }

        KeyCode::Char('>') => app.indent_line(),
        KeyCode::Char('<') => app.dedent_line(),

        _ => {}
    }

    false
}

fn handle_insert_mode(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char(' ') => {
                app.request_completion();
                return;
            }
            KeyCode::Char(']') => {
                app.request_definition();
                return;
            }
            _ => {}
        }
    }

    if !app.completions.is_empty() {
        match key.code {
            KeyCode::Esc => {
                app.completions.clear();
                return;
            }
            KeyCode::Tab => {
                app.accept_completion();
                return;
            }
            KeyCode::Up => {
                if !app.completions.is_empty() {
                    app.completion_index =
                        (app.completion_index + app.completions.len() - 1) % app.completions.len();
                }
                return;
            }
            KeyCode::Down => {
                if !app.completions.is_empty() {
                    app.completion_index = (app.completion_index + 1) % app.completions.len();
                }
                return;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.move_cursor_left();
        }
        KeyCode::Char(':') if is_text_key(&key) => {
            app.mode = AppMode::ExCommand;
            app.ex_command = ":".to_string();
        }
        KeyCode::Char(c) if is_text_key(&key) => app.insert_at_cursor(c),
        KeyCode::Enter => app.insert_newline(),
        KeyCode::Backspace => app.delete_backward(),
        KeyCode::Delete => app.delete_at_cursor(),
        KeyCode::Tab => {
            for _ in 0..app.tab_size {
                app.insert_at_cursor(' ');
            }
        }
        KeyCode::Home => app.move_cursor_to_line_start(),
        KeyCode::End => app.move_cursor_to_line_end(),
        _ => {}
    }
}

fn handle_visual_mode(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('v') => app.mode = AppMode::Normal,
            KeyCode::Char('y') => {
                app.yank_visual();
                app.mode = AppMode::Normal;
            }
            KeyCode::Char('d') | KeyCode::Char('x') => {
                app.delete_visual();
                app.mode = AppMode::Normal;
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Esc => app.mode = AppMode::Normal,
        KeyCode::Up => app.move_cursor_up(),
        KeyCode::Down => app.move_cursor_down(),
        KeyCode::Left => app.move_cursor_left(),
        KeyCode::Right => app.move_cursor_right(),
        KeyCode::Char('0') | KeyCode::Home => app.move_cursor_to_line_start(),
        KeyCode::Char('$') | KeyCode::End => app.move_cursor_to_line_end(),
        KeyCode::Char('^') => app.move_to_first_non_blank(),
        KeyCode::PageUp => {
            for _ in 0..10 {
                app.move_cursor_up();
            }
        }
        KeyCode::PageDown => {
            for _ in 0..10 {
                app.move_cursor_down();
            }
        }
        KeyCode::Char(':') => {
            app.mode = AppMode::ExCommand;
            app.ex_command = ":'<,'>".to_string();
        }
        _ => {}
    }
}

fn handle_search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.mode = AppMode::Normal,
        KeyCode::Enter => {
            let query = app.search_query.clone();
            app.search(&query);
            app.mode = AppMode::Normal;
        }
        KeyCode::Backspace => {
            app.search_query.pop();
        }
        KeyCode::Char(c) if is_text_key(&key) => app.search_query.push(c),
        _ => {}
    }
}

fn handle_ex_mode(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.ex_command.clear();
        }
        KeyCode::Enter => {
            let cmd = app.ex_command.clone();
            app.execute_ex_command(&cmd);
            app.ex_command.clear();
            if !app.running {
                return true;
            }
            if app.mode != AppMode::HelpMenu {
                app.mode = AppMode::Normal;
            }
        }
        KeyCode::Backspace => {
            if app.ex_command.len() > 1 {
                app.ex_command.pop();
            }
        }
        KeyCode::Char(c) if is_text_key(&key) => app.ex_command.push(c),
        KeyCode::Up => {
            if let Some(cmd) = app.command_history.back() {
                app.ex_command = cmd.clone();
            }
        }
        KeyCode::Down => app.ex_command = ":".to_string(),
        _ => {}
    }
    false
}

fn handle_help_menu(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => {
            app.mode = AppMode::Normal;
            app.set_status("");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn plain_letters_are_text_not_commands() {
        let mut app = App::new();
        assert_eq!(app.mode, AppMode::Insert);
        for letter in ['q', 'a', 'i', 'v', 'd', 'w', 'x'] {
            handle_key(&mut app, press(KeyCode::Char(letter)));
            assert_eq!(app.mode, AppMode::Insert);
        }
        assert_eq!(app.text_buffer[0], "qaivdwx");
    }

    #[test]
    fn enter_and_ctrl_q_replace_i_and_q() {
        let mut app = App::new();
        handle_key(&mut app, press(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Normal);
        handle_key(&mut app, press(KeyCode::Enter));
        assert_eq!(app.mode, AppMode::Insert);

        let quit = handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
        );
        assert!(quit);
    }
}
