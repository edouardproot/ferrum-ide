use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "try",
];

const TYPES: &[&str] = &[
    "bool", "char", "str", "usize", "isize", "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32",
    "i64", "i128", "f32", "f64", "String", "Vec", "Option", "Result", "HashMap", "BTreeMap", "Box",
    "Rc", "Arc", "Cell", "RefCell", "Path", "PathBuf",
];

fn style_for(token: &str, kind: Kind) -> Style {
    match kind {
        Kind::Keyword => Style::default()
            .fg(Color::Rgb(197, 134, 192))
            .add_modifier(Modifier::BOLD),
        Kind::Type => Style::default().fg(Color::Rgb(86, 182, 194)),
        Kind::String => Style::default().fg(Color::Rgb(152, 195, 121)),
        Kind::Comment => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
        Kind::Number => Style::default().fg(Color::Rgb(209, 154, 102)),
        Kind::Macro => Style::default().fg(Color::Rgb(220, 220, 170)),
        Kind::Attribute => Style::default().fg(Color::Gray),
        Kind::Plain => {
            if KEYWORDS.contains(&token) {
                return style_for(token, Kind::Keyword);
            }
            if TYPES.contains(&token) {
                return style_for(token, Kind::Type);
            }
            Style::default().fg(Color::White)
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    Keyword,
    Type,
    String,
    Comment,
    Number,
    Macro,
    Attribute,
}

fn push(parts: &mut Vec<Span<'static>>, text: String, kind: Kind) {
    if text.is_empty() {
        return;
    }
    let style = if matches!(kind, Kind::Plain) {
        if KEYWORDS.contains(&text.as_str()) {
            style_for("", Kind::Keyword)
        } else if TYPES.contains(&text.as_str()) {
            style_for("", Kind::Type)
        } else {
            style_for("", kind)
        }
    } else {
        style_for("", kind)
    };
    parts.push(Span::styled(text, style));
}

fn char_literal_end(line: &str, start: usize) -> Option<usize> {
    let rest = line.get(start + 1..)?;
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    if first == '\\' {
        let mut escaped = true;
        for (idx, c) in chars {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
            } else if c == '\'' {
                return Some(start + 1 + idx + 1);
            }
        }
        None
    } else if first != '\'' {
        let (idx, second) = chars.next()?;
        if second == '\'' {
            Some(start + 1 + idx + 1)
        } else {
            None
        }
    } else {
        None
    }
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub fn highlight(line: &str) -> Vec<Span<'static>> {
    let mut parts = Vec::new();
    let mut iter = line.char_indices().peekable();

    while let Some((start, ch)) = iter.next() {
        if ch == '/' && matches!(iter.peek(), Some((_, '/'))) {
            iter.next();
            let rest = line[start..].to_string();
            push(&mut parts, rest, Kind::Comment);
            break;
        }

        if ch == '#' && matches!(iter.peek(), Some((_, '['))) {
            iter.next();
            let mut end = line.len();
            let mut depth = 1usize;
            for (idx, c) in line[start + 2..].char_indices() {
                match c {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = start + 2 + idx + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            push(&mut parts, line[start..end].to_string(), Kind::Attribute);
            while iter.peek().map(|(i, _)| *i < end).unwrap_or(false) {
                iter.next();
            }
            continue;
        }

        if ch == '"' {
            let mut escaped = false;
            let mut end = line.len();
            for (idx, c) in line[start + 1..].char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    end = start + 1 + idx + 1;
                    break;
                }
            }
            push(&mut parts, line[start..end].to_string(), Kind::String);
            while iter.peek().map(|(i, _)| *i < end).unwrap_or(false) {
                iter.next();
            }
            continue;
        }

        if ch == '\'' {
            if let Some(end) = char_literal_end(line, start) {
                push(&mut parts, line[start..end].to_string(), Kind::String);
                while iter.peek().map(|(i, _)| *i < end).unwrap_or(false) {
                    iter.next();
                }
                continue;
            }
            let mut end = start + ch.len_utf8();
            for (idx, c) in line[end..].char_indices() {
                if is_ident_char(c) {
                    end = start + ch.len_utf8() + idx + c.len_utf8();
                } else {
                    break;
                }
            }
            push(&mut parts, line[start..end].to_string(), Kind::Type);
            while iter.peek().map(|(i, _)| *i < end).unwrap_or(false) {
                iter.next();
            }
            continue;
        }

        if ch.is_ascii_digit() {
            let mut end = start + ch.len_utf8();
            for (idx, c) in line[end..].char_indices() {
                let abs = start + ch.len_utf8() + idx;
                if c.is_ascii_alphanumeric()
                    || c == '_'
                    || (c == '.'
                        && !line[abs..].starts_with("..")
                        && line[abs + 1..]
                            .chars()
                            .next()
                            .is_some_and(|n| n.is_ascii_digit()))
                {
                    end = abs + c.len_utf8();
                } else {
                    break;
                }
            }
            push(&mut parts, line[start..end].to_string(), Kind::Number);
            while iter.peek().map(|(i, _)| *i < end).unwrap_or(false) {
                iter.next();
            }
            continue;
        }

        if ch.is_alphabetic() || ch == '_' {
            let mut end = start + ch.len_utf8();
            for (idx, c) in line[end..].char_indices() {
                if is_ident_char(c) {
                    end = start + ch.len_utf8() + idx + c.len_utf8();
                } else {
                    break;
                }
            }
            let token = &line[start..end];
            if line[end..].starts_with('!') {
                end += 1;
                push(&mut parts, format!("{}!", token), Kind::Macro);
            } else {
                push(&mut parts, token.to_string(), Kind::Plain);
            }
            while iter.peek().map(|(i, _)| *i < end).unwrap_or(false) {
                iter.next();
            }
            continue;
        }

        push(&mut parts, ch.to_string(), Kind::Plain);
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn macros_do_not_duplicate_bang() {
        let line = "println!(\"hello {}\", x);";
        let spans = highlight(line);
        assert_eq!(plain_text(&spans), line);
        assert_eq!(spans[0].content.as_ref(), "println!");
    }

    #[test]
    fn lifetimes_do_not_bleed_into_strings() {
        let line = "fn foo<'a, 'b>(x: &'a str) -> &'b str { x }";
        let spans = highlight(line);
        assert_eq!(plain_text(&spans), line);
        let string_color = style_for("", Kind::String).fg;
        assert!(!spans.iter().any(|s| s.style.fg == string_color));
    }

    #[test]
    fn char_literals_and_ranges_highlight_cleanly() {
        let line = "let c = '\\n'; let r = 1..5; let f = 3.14;";
        let spans = highlight(line);
        assert_eq!(plain_text(&spans), line);
        assert!(spans.iter().any(|s| s.content.as_ref() == "'\\n'"));
        assert!(spans.iter().any(|s| s.content.as_ref() == "3.14"));
        assert!(!spans.iter().any(|s| s.content.as_ref() == "1..5"));
    }
}
