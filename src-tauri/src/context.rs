use std::path::{Path, PathBuf};
use std::process::Command;

pub fn collect_file_tree(project_root: &Path) -> Option<String> {
    if !project_root.is_dir() {
        return None;
    }

    let output = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(project_root)
        .output()
        .ok()?;

    if output.status.success() {
        let files = String::from_utf8_lossy(&output.stdout);
        let truncated: String = files
            .lines()
            .take(200)
            .collect::<Vec<_>>()
            .join("\n");
        return Some(truncated);
    }

    let mut files = Vec::new();
    collect_recursive(project_root, project_root, 0, 2, &mut files);
    files.truncate(200);
    Some(files.join("\n"))
}

fn collect_recursive(
    base: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().to_string());
        }

        if path.is_dir() {
            collect_recursive(base, &path, depth + 1, max_depth, out);
        }
    }
}

pub fn detect_project_root() -> Option<PathBuf> {
    let output = Command::new("sh")
        .args([
            "-c",
            "lsof -p $(osascript -e 'tell application \"System Events\" to unix id of first process whose frontmost is true') 2>/dev/null | grep cwd | awk '{print $NF}'"
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let cwd = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !cwd.is_empty() && Path::new(&cwd).is_dir() {
            return Some(PathBuf::from(cwd));
        }
    }

    None
}
