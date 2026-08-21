//! Guardrail for keeping AI-touchable source files small enough to reason about.
//!
//! Mirrors the Marbles / agg-gui / instant-astronomer policy: every
//! first-party file in the workspace stays at or below 800 lines. When this
//! test fails, split the offending file into focused sibling modules instead
//! of compressing whitespace / comments to slip under the cap.

use std::fs;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 800;

const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    ".cursor",
    ".claude",
    "target",
    // The pinned Swift source being ported — upstream's file sizes are not
    // ours to police (SessionEngine.swift alone is ~1400 lines; its Rust
    // port must be split into modules).
    "keyinsight-swift-reference",
    // verovio-rust is its own repo (LGPL) pinned as a submodule; its
    // file sizes are policed there.
    "verovio-rust",
    "keyinsight-wasm/pkg",
    "demo/node_modules",
    "demo/dist",
    "demo/public/pkg",
];

const CHECKED_EXTENSIONS: &[&str] = &[
    "css", "html", "js", "json", "md", "rs", "toml", "ts", "tsx", "yaml", "yml",
];

#[test]
fn first_party_project_files_stay_under_line_limit() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("keyinsight-core should live under the workspace root");

    let mut offenders = Vec::new();
    visit_files(workspace_root, workspace_root, &mut offenders);

    if !offenders.is_empty() {
        offenders.sort();
        panic!(
            "project files must stay at or below {MAX_LINES} lines; offenders:\n{}",
            offenders
                .into_iter()
                .map(|(lines, path)| format!("{lines:>5}  {}", path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

fn visit_files(root: &Path, dir: &Path, offenders: &mut Vec<(usize, PathBuf)>) {
    if is_excluded(root, dir) {
        return;
    }

    let entries = fs::read_dir(dir).unwrap_or_else(|err| {
        panic!("failed to read directory {}: {err}", dir.display());
    });

    for entry in entries {
        let entry = entry.unwrap_or_else(|err| {
            panic!("failed to read directory entry in {}: {err}", dir.display());
        });
        let path = entry.path();
        let file_type = entry.file_type().unwrap_or_else(|err| {
            panic!("failed to read file type for {}: {err}", path.display());
        });

        if file_type.is_dir() {
            visit_files(root, &path, offenders);
        } else if file_type.is_file() && should_check_file(&path) {
            let lines = count_lines(&path);
            if lines > MAX_LINES {
                let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                offenders.push((lines, rel));
            }
        }
    }
}

fn is_excluded(root: &Path, path: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel = rel.to_string_lossy().replace('\\', "/");
    EXCLUDED_DIRS
        .iter()
        .any(|excluded| rel == *excluded || rel.starts_with(&format!("{excluded}/")))
}

fn should_check_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            CHECKED_EXTENSIONS
                .iter()
                .any(|checked| ext.eq_ignore_ascii_case(checked))
        })
        .unwrap_or(false)
}

fn count_lines(path: &Path) -> usize {
    let text = fs::read_to_string(path).unwrap_or_else(|err| {
        panic!("failed to read {} as UTF-8 text: {err}", path.display());
    });
    text.lines().count()
}
