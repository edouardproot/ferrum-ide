use ratatui::style::Color;
use std::fmt::Write;

#[derive(Debug, Clone, Default)]
pub struct TextFormat {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<Color>,
    pub bg_color: Option<Color>,
    pub font_scale: FontScale,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum FontScale {
    #[default]
    Normal,
    Small,
    Large,
    ExtraLarge,
}

pub struct TextPrinter {
    format: TextFormat,
    margin: usize,
    page_width: usize,
    line_spacing: usize,
    header: Option<String>,
    footer: Option<String>,
    print_timestamps: bool,
    print_line_numbers: bool,
    output_buffer: String,
}

impl TextPrinter {
    pub fn new() -> Self {
        Self {
            format: TextFormat::default(),
            margin: 2,
            page_width: 80,
            line_spacing: 0,
            header: None,
            footer: None,
            print_timestamps: false,
            print_line_numbers: true,
            output_buffer: String::new(),
        }
    }

    pub fn set_format(&mut self, format: TextFormat) {
        self.format = format;
    }

    #[allow(dead_code)]
    pub fn set_margin(&mut self, margin: usize) {
        self.margin = margin;
    }

    #[allow(dead_code)]
    pub fn set_page_width(&mut self, width: usize) {
        self.page_width = width;
    }

    #[allow(dead_code)]
    pub fn set_line_spacing(&mut self, spacing: usize) {
        self.line_spacing = spacing;
    }

    #[allow(dead_code)]
    pub fn set_header(&mut self, header: impl Into<String>) {
        self.header = Some(header.into());
    }

    #[allow(dead_code)]
    pub fn set_footer(&mut self, footer: impl Into<String>) {
        self.footer = Some(footer.into());
    }

    #[allow(dead_code)]
    pub fn toggle_timestamps(&mut self) {
        self.print_timestamps = !self.print_timestamps;
    }

    #[allow(dead_code)]
    pub fn toggle_line_numbers(&mut self) {
        self.print_line_numbers = !self.print_line_numbers;
    }

    pub fn print_line(&mut self, text: &str) -> String {
        let mut output = String::new();

        for _ in 0..self.margin {
            output.push(' ');
        }

        if self.format.bold {
            output.push_str("[B]");
        }
        if self.format.italic {
            output.push_str("[I]");
        }
        if self.format.underline {
            output.push_str("[U]");
        }
        if let Some(color) = self.format.color {
            output.push_str(&format!("[{:?}] ", color));
        }
        if let Some(bg) = self.format.bg_color {
            output.push_str(&format!("[BG:{:?}] ", bg));
        }
        if self.format.font_scale != FontScale::Normal {
            output.push_str(&format!("[{:?}] ", self.format.font_scale));
        }

        output.push_str(text);

        if self.format.font_scale != FontScale::Normal {
            output.push_str(&format!("[/{:?}]", self.format.font_scale));
        }
        if self.format.bg_color.is_some() {
            output.push_str("[/BG]");
        }
        if self.format.underline {
            output.push_str("[/U]");
        }
        if self.format.italic {
            output.push_str("[/I]");
        }
        if self.format.bold {
            output.push_str("[/B]");
        }

        output
    }

    pub fn print_buffer(&mut self, buffer: &[String]) -> String {
        self.output_buffer.clear();

        if let Some(ref header) = self.header {
            self.output_buffer.push_str(&self.format_header(header));
            self.output_buffer.push('\n');
        }

        let separator = self.format_separator();
        self.output_buffer.push_str(&separator);
        self.output_buffer.push('\n');

        for (idx, line) in buffer.iter().enumerate() {
            let mut printed_line = String::new();

            if self.print_line_numbers {
                let _ = write!(printed_line, " {:>4} |", idx + 1);
            }

            if self.print_timestamps {
                let _ = write!(printed_line, " [{}]", self.format_timestamp());
            }

            printed_line.push_str(&self.print_line(line));

            self.output_buffer.push_str(&printed_line);
            self.output_buffer.push('\n');

            for _ in 0..self.line_spacing {
                self.output_buffer.push('\n');
            }
        }

        self.output_buffer.push_str(&separator);
        self.output_buffer.push('\n');
        if let Some(ref footer) = self.footer {
            self.output_buffer.push_str(&self.format_footer(footer));
            self.output_buffer.push('\n');
        }

        let stats = self.format_stats(buffer);
        self.output_buffer.push_str(&stats);

        self.output_buffer.clone()
    }

    fn format_header(&self, header: &str) -> String {
        let mut output = String::new();
        let width = self.page_width.saturating_sub(self.margin * 2);

        let clipped: String = header.chars().take(width).collect();
        let padding = (width - clipped.chars().count()) / 2;

        for _ in 0..self.margin {
            output.push(' ');
        }
        for _ in 0..padding {
            output.push(' ');
        }
        output.push_str(&clipped);
        output
    }

    fn format_footer(&self, footer: &str) -> String {
        let mut output = String::new();
        for _ in 0..self.margin {
            output.push(' ');
        }
        output.push_str(footer);
        output
    }

    fn format_separator(&self) -> String {
        let mut output = String::new();
        for _ in 0..self.margin {
            output.push(' ');
        }
        for _ in 0..self.page_width.saturating_sub(self.margin * 2) {
            output.push('─');
        }
        output
    }

    fn format_stats(&self, buffer: &[String]) -> String {
        let lines = buffer.len();
        let words: usize = buffer.iter().map(|l| l.split_whitespace().count()).sum();
        let chars: usize = buffer.iter().map(|l| l.chars().count()).sum();

        let mut output = String::new();
        let _ = write!(
            output,
            "\nStatistics: {} lines, {} words, {} characters\n",
            lines, words, chars
        );
        output
    }

    fn format_timestamp(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = duration.as_secs();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    }

    #[allow(dead_code)]
    pub fn transform(&self, text: &str, transform: TransformType) -> String {
        match transform {
            TransformType::UpperCase => text.to_uppercase(),
            TransformType::LowerCase => text.to_lowercase(),
            TransformType::TitleCase => self.to_title_case(text),
            TransformType::Reverse => text.chars().rev().collect(),
            TransformType::Trim => text.trim().to_string(),
            TransformType::RemoveEmptyLines => text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            TransformType::SortLines => {
                let mut lines: Vec<&str> = text.lines().collect();
                lines.sort();
                lines.join("\n")
            }
            TransformType::UniqueLines => {
                let mut lines: Vec<&str> = text.lines().collect();
                lines.sort();
                lines.dedup();
                lines.join("\n")
            }
        }
    }

    fn to_title_case(&self, text: &str) -> String {
        text.split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        let upper: String = first.to_uppercase().collect();
                        let rest: String = chars.flat_map(|c| c.to_lowercase()).collect();
                        format!("{}{}", upper, rest)
                    }
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[allow(dead_code)]
    pub fn generate_banner(&self, text: &str, style: BannerStyle) -> Vec<String> {
        let mut banner = Vec::new();
        let width = text.chars().count() + self.margin * 2;

        match style {
            BannerStyle::Box => {
                banner.push(format!("┌{}┐", "─".repeat(width)));
                banner.push(format!("│ {:<width$} │", text, width = width));
                banner.push(format!("└{}┘", "─".repeat(width)));
            }
            BannerStyle::Double => {
                banner.push(format!("╔{}╗", "═".repeat(width)));
                banner.push(format!("║ {:<width$} ║", text, width = width));
                banner.push(format!("╚{}╝", "═".repeat(width)));
            }
            BannerStyle::Arrow => {
                banner.push(format!("> {:<width$} <", text, width = width));
            }
            BannerStyle::Dashed => {
                banner.push(format!("-- {:<width$} --", text, width = width));
            }
        }

        banner
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformType {
    UpperCase,
    LowerCase,
    TitleCase,
    Reverse,
    Trim,
    RemoveEmptyLines,
    SortLines,
    UniqueLines,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BannerStyle {
    Box,
    Double,
    Arrow,
    Dashed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_line() {
        let mut printer = TextPrinter::new();
        let result = printer.print_line("Hello");
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_transform_uppercase() {
        let printer = TextPrinter::new();
        let result = printer.transform("hello world", TransformType::UpperCase);
        assert_eq!(result, "HELLO WORLD");
    }

    #[test]
    fn test_transform_title_case() {
        let printer = TextPrinter::new();
        let result = printer.transform("hello world", TransformType::TitleCase);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_transform_reverse() {
        let printer = TextPrinter::new();
        let result = printer.transform("hello", TransformType::Reverse);
        assert_eq!(result, "olleh");
    }

    #[test]
    fn test_banner_generation() {
        let printer = TextPrinter::new();
        let banner = printer.generate_banner("Test", BannerStyle::Box);
        assert_eq!(banner.len(), 3);
        assert!(banner[0].contains("┌"));
        assert!(banner[2].contains("└"));
    }

    #[test]
    fn test_print_buffer_stats() {
        let mut printer = TextPrinter::new();
        let buffer = vec![String::from("line1"), String::from("line2")];
        let output = printer.print_buffer(&buffer);
        assert!(output.contains("Statistics:"));
        assert!(output.contains("2 lines"));
    }
}
