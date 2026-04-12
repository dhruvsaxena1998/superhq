use gpui::{HighlightStyle, Rgba};
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::sync::LazyLock;
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};
use tree_sitter_language::LanguageFn;

// ── Syntax theme (loaded from JSON) ────────────────────────────

/// Maps tree-sitter capture names to colors.
/// Loaded once from the embedded JSON file.
static SYNTAX_THEME: LazyLock<HashMap<String, HighlightStyle>> = LazyLock::new(|| {
    let raw: HashMap<String, String> =
        serde_json::from_str(include_str!("../../../assets/themes/superhq-dark-syntax.json"))
            .expect("Failed to parse syntax theme (superhq-dark-syntax.json)");

    raw.into_iter()
        .filter_map(|(name, hex)| {
            let color = parse_hex(&hex)?;
            Some((
                name,
                HighlightStyle {
                    color: Some(color.into()),
                    ..Default::default()
                },
            ))
        })
        .collect()
});

fn parse_hex(hex: &str) -> Option<Rgba> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    })
}

fn style_for_capture(name: &str) -> Option<HighlightStyle> {
    // Try exact match first, then progressively strip suffixes.
    // e.g. "keyword.function" → "keyword.function", then "keyword"
    let theme = &*SYNTAX_THEME;
    if let Some(style) = theme.get(name) {
        return Some(*style);
    }
    // Walk up the capture hierarchy
    let mut key = name;
    while let Some(dot) = key.rfind('.') {
        key = &key[..dot];
        if let Some(style) = theme.get(key) {
            return Some(*style);
        }
    }
    None
}

// ── Language registry ──────────────────────────────────────────

fn get_language(name: &str) -> Option<(LanguageFn, &'static str)> {
    match name {
        "rust" => Some((tree_sitter_rust::LANGUAGE, tree_sitter_rust::HIGHLIGHTS_QUERY)),
        "javascript" => Some((tree_sitter_javascript::LANGUAGE, tree_sitter_javascript::HIGHLIGHT_QUERY)),
        "typescript" => Some((tree_sitter_typescript::LANGUAGE_TYPESCRIPT, tree_sitter_typescript::HIGHLIGHTS_QUERY)),
        "python" => Some((tree_sitter_python::LANGUAGE, tree_sitter_python::HIGHLIGHTS_QUERY)),
        "go" => Some((tree_sitter_go::LANGUAGE, tree_sitter_go::HIGHLIGHTS_QUERY)),
        "java" => Some((tree_sitter_java::LANGUAGE, tree_sitter_java::HIGHLIGHTS_QUERY)),
        "c" => Some((tree_sitter_c::LANGUAGE, tree_sitter_c::HIGHLIGHT_QUERY)),
        "cpp" => Some((tree_sitter_cpp::LANGUAGE, tree_sitter_cpp::HIGHLIGHT_QUERY)),
        "ruby" => Some((tree_sitter_ruby::LANGUAGE, tree_sitter_ruby::HIGHLIGHTS_QUERY)),
        "swift" => Some((tree_sitter_swift::LANGUAGE, tree_sitter_swift::HIGHLIGHTS_QUERY)),
        "scala" => Some((tree_sitter_scala::LANGUAGE, tree_sitter_scala::HIGHLIGHTS_QUERY)),
        "zig" => Some((tree_sitter_zig::LANGUAGE, tree_sitter_zig::HIGHLIGHTS_QUERY)),
        "bash" => Some((tree_sitter_bash::LANGUAGE, tree_sitter_bash::HIGHLIGHT_QUERY)),
        "html" => Some((tree_sitter_html::LANGUAGE, tree_sitter_html::HIGHLIGHTS_QUERY)),
        "css" => Some((tree_sitter_css::LANGUAGE, tree_sitter_css::HIGHLIGHTS_QUERY)),
        "json" => Some((tree_sitter_json::LANGUAGE, tree_sitter_json::HIGHLIGHTS_QUERY)),
        "toml" => Some((tree_sitter_toml_ng::LANGUAGE, tree_sitter_toml_ng::HIGHLIGHTS_QUERY)),
        "yaml" => Some((tree_sitter_yaml::LANGUAGE, tree_sitter_yaml::HIGHLIGHTS_QUERY)),
        "markdown" => Some((tree_sitter_md::LANGUAGE, tree_sitter_md::HIGHLIGHT_QUERY_BLOCK)),
        "sql" => Some((tree_sitter_sequel::LANGUAGE, tree_sitter_sequel::HIGHLIGHTS_QUERY)),
        "elixir" => Some((tree_sitter_elixir::LANGUAGE, tree_sitter_elixir::HIGHLIGHTS_QUERY)),
        _ => None,
    }
}

// ── Public API ─────────────────────────────────────────────────

/// Run tree-sitter highlighting on `text` for the given language name.
/// Returns styled ranges suitable for mapping to GPUI TextRuns.
pub fn highlight(language_name: &str, text: &str) -> Vec<(Range<usize>, HighlightStyle)> {
    let Some((lang_fn, query_src)) = get_language(language_name) else {
        return Vec::new();
    };

    let lang: Language = lang_fn.into();
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Vec::new();
    };

    let query = match Query::new(&lang, query_src) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let capture_names: Vec<&str> = query.capture_names().iter().map(|s| &**s).collect();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), text.as_bytes());
    let mut raw: Vec<(Range<usize>, HighlightStyle)> = Vec::new();

    while let Some(m) = matches.next() {
        for cap in m.captures {
            let name = capture_names[cap.index as usize];
            if let Some(style) = style_for_capture(name) {
                let range = cap.node.byte_range();
                if range.start < range.end {
                    raw.push((range, style));
                }
            }
        }
    }

    if raw.is_empty() {
        return Vec::new();
    }

    merge_styles(&(0..text.len()), raw)
}

/// Merge overlapping highlight ranges. Later (more specific) captures
/// override earlier ones where they overlap.
fn merge_styles(
    total_range: &Range<usize>,
    styles: Vec<(Range<usize>, HighlightStyle)>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut boundaries = BTreeSet::new();
    boundaries.insert(total_range.start);
    boundaries.insert(total_range.end);
    for (range, _) in &styles {
        boundaries.insert(range.start);
        boundaries.insert(range.end);
    }

    let pts: Vec<usize> = boundaries.into_iter().collect();
    let mut result = Vec::with_capacity(pts.len());

    for i in 0..pts.len().saturating_sub(1) {
        let interval = pts[i]..pts[i + 1];
        if interval.start >= interval.end {
            continue;
        }

        let mut top: Option<HighlightStyle> = None;
        for (range, style) in &styles {
            if range.start <= interval.start && interval.end <= range.end {
                top = Some(*style);
            }
        }

        if let Some(style) = top {
            result.push((interval, style));
        }
    }

    let mut merged: Vec<(Range<usize>, HighlightStyle)> = Vec::with_capacity(result.len());
    for (range, style) in result {
        if let Some((last_range, last_style)) = merged.last_mut() {
            if last_range.end == range.start && *last_style == style {
                last_range.end = range.end;
                continue;
            }
        }
        merged.push((range, style));
    }

    merged
}
