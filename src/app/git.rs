use std::path::Path;
use std::process::Command;

pub fn status(cwd: &Path) -> (String, Vec<String>) {
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output();

    let branch = match branch_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            let sym_output = Command::new("git")
                .args(["symbolic-ref", "--short", "HEAD"])
                .current_dir(cwd)
                .output();
            match sym_output {
                Ok(output) if output.status.success() => {
                    String::from_utf8_lossy(&output.stdout).trim().to_string()
                }
                _ => "(no git repository)".to_string(),
            }
        }
    };

    let output = Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(cwd)
        .output();

    let Ok(output) = output else {
        return (branch, vec!["git status failed".to_string()]);
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "git status failed".to_string()
        } else {
            stderr
        };
        return (branch, vec![message]);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let lines = text
        .lines()
        .map(|line| {
            let status: String = line.chars().take(2).collect();
            let path = line.get(status.len() + 1..).unwrap_or("");
            format!("{} {}", status.trim_end(), path)
        })
        .filter(|line| !line.trim().is_empty())
        .collect();

    (branch, lines)
}
