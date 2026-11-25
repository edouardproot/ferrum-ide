use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct EditorBuffer {
    pub path: Option<PathBuf>,
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
    pub scroll_offset: usize,
    pub modified: bool,
    pub trailing_newline: bool,
}

impl EditorBuffer {
    pub fn untitled() -> Self {
        Self {
            path: None,
            lines: vec![String::new()],
            cursor: (0, 0),
            scroll_offset: 0,
            modified: false,
            trailing_newline: true,
        }
    }

    pub fn from_path(path: &Path, lines: Vec<String>, trailing_newline: bool) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            lines,
            cursor: (0, 0),
            scroll_offset: 0,
            modified: false,
            trailing_newline,
        }
    }

    pub fn label(&self) -> String {
        match &self.path {
            Some(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            None => "[No Name]".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExplorerEntry {
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
}

pub struct Explorer {
    pub root: PathBuf,
    pub entries: Vec<ExplorerEntry>,
    pub selected: usize,
}

impl Explorer {
    pub fn new(root: PathBuf) -> Self {
        let mut explorer = Self {
            root,
            entries: Vec::new(),
            selected: 0,
        };
        explorer.refresh();
        explorer
    }

    pub fn refresh(&mut self) {
        let expanded: HashSet<PathBuf> = self
            .entries
            .iter()
            .filter(|entry| entry.is_dir && entry.expanded)
            .map(|entry| entry.path.clone())
            .collect();

        self.entries.clear();
        self.collect(&self.root.clone(), 0, &expanded, 0);
        if self.entries.is_empty() {
            self.entries.push(ExplorerEntry {
                path: self.root.clone(),
                depth: 0,
                is_dir: true,
                expanded: false,
            });
        }
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }

    fn collect(&mut self, dir: &Path, depth: usize, expanded: &HashSet<PathBuf>, guard: usize) {
        if depth > 8 || guard > 4000 {
            return;
        }

        let Ok(read_dir) = fs::read_dir(dir) else {
            return;
        };

        let mut children = Vec::new();
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.')
                || matches!(name.as_str(), "target" | "node_modules" | "__pycache__")
            {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            children.push((name, path, file_type.is_dir()));
        }
        children.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });

        for (_, path, is_dir) in children {
            let is_expanded = is_dir && expanded.contains(&path);
            self.entries.push(ExplorerEntry {
                path: path.clone(),
                depth,
                is_dir,
                expanded: is_expanded,
            });
            if is_expanded {
                self.collect(&path, depth + 1, expanded, guard + 1);
            }
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as isize;
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, len - 1) as usize;
    }

    pub fn toggle(&mut self) {
        if let Some(entry) = self.entries.get_mut(self.selected) {
            if entry.is_dir {
                entry.expanded = !entry.expanded;
            }
        }
        self.refresh();
    }

    pub fn parent(&mut self) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        if entry.depth == 0 {
            return;
        }
        let target_depth = entry.depth.saturating_sub(1);
        if let Some(index) = self.entries[..self.selected]
            .iter()
            .rposition(|candidate| candidate.depth == target_depth && candidate.is_dir)
        {
            self.selected = index;
        }
    }

    pub fn selected_entry(&self) -> Option<&ExplorerEntry> {
        self.entries.get(self.selected)
    }
}
