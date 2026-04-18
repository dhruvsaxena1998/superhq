use shuru_sdk::AsyncSandbox;
use similar::{ChangeTag, TextDiff};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
}

#[derive(Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    pub content: String,
}

#[derive(Clone)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone)]
pub struct FileDiff {
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
}

impl FileDiff {
    pub fn is_empty(&self) -> bool {
        !self.is_binary && self.additions == 0 && self.deletions == 0
    }
}

#[derive(Clone, Default)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
}

/// Compute effective stats given partial staging. Discarded lines don't
/// count. For replacement pairs, both the addition and deletion only
/// count when the addition is kept.
pub fn effective_stats(hunks: &[DiffHunk], discarded: &HashSet<(usize, usize)>) -> DiffStats {
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for (hunk_idx, hunk) in hunks.iter().enumerate() {
        let pairs = find_replacement_pairs(&hunk.lines);
        let paired_del_set: HashSet<usize> = pairs.iter().map(|(d, _)| *d).collect();
        let add_to_del: HashMap<usize, usize> = pairs.iter().map(|(d, a)| (*a, *d)).collect();

        for (line_idx, line) in hunk.lines.iter().enumerate() {
            match line.kind {
                DiffLineKind::Context => {}
                DiffLineKind::Addition => {
                    if !discarded.contains(&(hunk_idx, line_idx)) {
                        additions += 1;
                        if add_to_del.contains_key(&line_idx) {
                            deletions += 1;
                        }
                    }
                }
                DiffLineKind::Deletion => {
                    if !paired_del_set.contains(&line_idx)
                        && !discarded.contains(&(hunk_idx, line_idx))
                    {
                        deletions += 1;
                    }
                }
            }
        }
    }

    DiffStats { additions, deletions }
}

pub async fn read_host_file(
    rel_path: &str,
    host_mount_path: &str,
) -> Option<Vec<u8>> {
    let full_path = format!("{}/{}", host_mount_path, rel_path);
    tokio::fs::read(&full_path).await.ok()
}

pub async fn read_sandbox_file(
    rel_path: &str,
    workspace: &str,
    sandbox: &Arc<AsyncSandbox>,
) -> Option<Vec<u8>> {
    let full_path = format!("{}/{}", workspace, rel_path);
    sandbox.read_file(&full_path).await.ok()
}

pub async fn copy_to_host(
    rel_path: &str,
    host_mount_path: &str,
    sandbox: &Arc<AsyncSandbox>,
) -> Result<(), String> {
    // Safety: reject traversal attacks
    if rel_path.contains("..") {
        return Err("path contains '..'".into());
    }
    // Safety: don't overwrite git internals
    if rel_path.starts_with(".git/") || rel_path == ".git" {
        return Err("refusing to write to .git".into());
    }

    let sandbox_path = format!("/workspace/{}", rel_path);
    let host_path = format!("{}/{}", host_mount_path, rel_path);

    let content = sandbox.read_file(&sandbox_path).await
        .map_err(|e| format!("read sandbox file: {e}"))?;

    if let Some(parent) = std::path::Path::new(&host_path).parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| format!("create dirs: {e}"))?;
    }

    tokio::fs::write(&host_path, &content).await
        .map_err(|e| format!("write host file: {e}"))?;

    Ok(())
}

fn is_binary(data: &[u8]) -> bool {
    let check_len = data.len().min(8192);
    data[..check_len].contains(&0)
}

/// Stats-only variant of `compute_file_diff`. Runs the same line diff but
/// doesn't materialize `DiffHunk`/`DiffLine` vecs — use when the caller only
/// needs addition/deletion counts (e.g. header totals for unexpanded rows).
pub fn compute_file_stats(old: &[u8], new: &[u8]) -> (DiffStats, bool) {
    if is_binary(old) || is_binary(new) {
        return (DiffStats::default(), true);
    }

    let old_text = String::from_utf8_lossy(old);
    let new_text = String::from_utf8_lossy(new);
    let diff = TextDiff::from_lines(old_text.as_ref(), new_text.as_ref());

    let mut additions = 0usize;
    let mut deletions = 0usize;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => additions += 1,
            ChangeTag::Delete => deletions += 1,
            ChangeTag::Equal => {}
        }
    }
    (DiffStats { additions, deletions }, false)
}

pub fn compute_file_diff(old: &[u8], new: &[u8]) -> FileDiff {
    if is_binary(old) || is_binary(new) {
        return FileDiff {
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
            is_binary: true,
        };
    }

    let old_text = String::from_utf8_lossy(old);
    let new_text = String::from_utf8_lossy(new);

    let diff = TextDiff::from_lines(old_text.as_ref(), new_text.as_ref());
    let mut hunks = Vec::new();
    let mut total_add = 0usize;
    let mut total_del = 0usize;

    for group in diff.grouped_ops(1) {
        let mut hunk_lines = Vec::new();
        let mut old_start = 0;
        let mut new_start = 0;
        let mut old_count = 0;
        let mut new_count = 0;
        let mut first = true;

        for op in &group {
            for change in diff.iter_changes(op) {
                let old_ln = change.old_index().map(|i| i + 1);
                let new_ln = change.new_index().map(|i| i + 1);

                if first {
                    old_start = old_ln.unwrap_or(1);
                    new_start = new_ln.unwrap_or(1);
                    first = false;
                }

                let kind = match change.tag() {
                    ChangeTag::Equal => {
                        old_count += 1;
                        new_count += 1;
                        DiffLineKind::Context
                    }
                    ChangeTag::Insert => {
                        new_count += 1;
                        total_add += 1;
                        DiffLineKind::Addition
                    }
                    ChangeTag::Delete => {
                        old_count += 1;
                        total_del += 1;
                        DiffLineKind::Deletion
                    }
                };

                hunk_lines.push(DiffLine {
                    kind,
                    old_lineno: old_ln,
                    new_lineno: new_ln,
                    content: change.to_string_lossy().to_string(),
                });
            }
        }

        hunks.push(DiffHunk {
            old_start,
            old_count,
            new_start,
            new_count,
            lines: hunk_lines,
        });
    }

    FileDiff {
        additions: total_add,
        deletions: total_del,
        hunks,
        is_binary: false,
    }
}

/// Within a hunk, find replacement pairs: consecutive deletion lines
/// immediately followed by consecutive addition lines form a replacement
/// block. Returns `(del_line_idx, add_line_idx)` pairs, matched 1:1.
/// Excess deletions or additions beyond the shorter side are unpaired.
pub fn find_replacement_pairs(lines: &[DiffLine]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].kind != DiffLineKind::Deletion {
            i += 1;
            continue;
        }

        let del_start = i;
        while i < lines.len() && lines[i].kind == DiffLineKind::Deletion {
            i += 1;
        }
        let del_count = i - del_start;

        if i >= lines.len() || lines[i].kind != DiffLineKind::Addition {
            continue;
        }

        let add_start = i;
        while i < lines.len() && lines[i].kind == DiffLineKind::Addition {
            i += 1;
        }
        let add_count = i - add_start;

        let pair_count = del_count.min(add_count);
        for j in 0..pair_count {
            pairs.push((del_start + j, add_start + j));
        }
    }

    pairs
}

/// Reconstruct a file by partially applying a diff.
///
/// Replacement pairs (deletion+addition that represent a line change) are
/// treated atomically: only the addition's discard state matters. If the
/// addition is kept, the new line replaces the old. If discarded, the old
/// line is preserved. The paired deletion has no independent toggle.
///
/// Unpaired lines follow simple rules:
/// - discarded Addition  -> omitted
/// - discarded Deletion  -> restored (original line kept)
/// - kept Addition       -> included
/// - kept Deletion       -> omitted (line stays deleted)
/// - Context             -> always included
pub fn reconstruct_partial(
    old: &str,
    hunks: &[DiffHunk],
    discarded: &HashSet<(usize, usize)>,
) -> String {
    let old_lines: Vec<&str> = old.split('\n').collect();
    let mut result: Vec<&str> = Vec::new();
    let mut old_pos: usize = 0;

    for (hunk_idx, hunk) in hunks.iter().enumerate() {
        let hunk_old_start = if hunk.old_start > 0 { hunk.old_start - 1 } else { 0 };

        while old_pos < hunk_old_start && old_pos < old_lines.len() {
            result.push(old_lines[old_pos]);
            old_pos += 1;
        }

        let pairs = find_replacement_pairs(&hunk.lines);
        let paired_del_set: HashSet<usize> = pairs.iter().map(|(d, _)| *d).collect();
        let add_to_del: HashMap<usize, usize> = pairs.iter().map(|(d, a)| (*a, *d)).collect();
        let mut saved_old: HashMap<usize, &str> = HashMap::new();

        for (line_idx, line) in hunk.lines.iter().enumerate() {
            match line.kind {
                DiffLineKind::Context => {
                    result.push(old_lines.get(old_pos).copied().unwrap_or(""));
                    old_pos += 1;
                }
                DiffLineKind::Deletion => {
                    if paired_del_set.contains(&line_idx) {
                        saved_old.insert(line_idx, old_lines.get(old_pos).copied().unwrap_or(""));
                        old_pos += 1;
                    } else {
                        let is_discarded = discarded.contains(&(hunk_idx, line_idx));
                        if is_discarded {
                            result.push(old_lines.get(old_pos).copied().unwrap_or(""));
                        }
                        old_pos += 1;
                    }
                }
                DiffLineKind::Addition => {
                    let is_discarded = discarded.contains(&(hunk_idx, line_idx));
                    if let Some(&del_idx) = add_to_del.get(&line_idx) {
                        if is_discarded {
                            if let Some(old_line) = saved_old.get(&del_idx) {
                                result.push(old_line);
                            }
                        } else {
                            result.push(line.content.trim_end_matches('\n'));
                        }
                    } else {
                        if !is_discarded {
                            result.push(line.content.trim_end_matches('\n'));
                        }
                    }
                }
            }
        }
    }

    while old_pos < old_lines.len() {
        result.push(old_lines[old_pos]);
        old_pos += 1;
    }

    result.join("\n")
}

pub async fn write_to_both(
    rel_path: &str,
    content: &[u8],
    host_mount_path: &str,
    sandbox: &Arc<AsyncSandbox>,
) -> Result<(), String> {
    if rel_path.contains("..") {
        return Err("path contains '..'".into());
    }
    if rel_path.starts_with(".git/") || rel_path == ".git" {
        return Err("refusing to write to .git".into());
    }

    let host_path = format!("{}/{}", host_mount_path, rel_path);
    let sandbox_path = format!("/workspace/{}", rel_path);

    if let Some(parent) = std::path::Path::new(&host_path).parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| format!("create dirs: {e}"))?;
    }

    tokio::fs::write(&host_path, content).await
        .map_err(|e| format!("write host file: {e}"))?;

    sandbox.write_file(&sandbox_path, content).await
        .map_err(|e| format!("write sandbox file: {e}"))?;

    Ok(())
}

