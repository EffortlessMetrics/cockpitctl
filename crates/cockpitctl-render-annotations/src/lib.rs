//! Deterministic annotation rendering for cockpitctl highlights.

#![warn(missing_docs)]

use cockpitctl_types::{CockpitConfig, Highlight, Severity, severity_rank};

/// Result of annotation rendering, tracking whether truncation occurred.
pub struct AnnotationRenderResult {
    /// The rendered markdown content for annotations.
    pub content: String,
    /// Whether the annotations were truncated due to max_annotations cap.
    pub truncated: bool,
    /// Total number of annotations before truncation.
    pub total_count: usize,
    /// Number of annotations actually rendered.
    pub rendered_count: usize,
}

/// Result of GitHub annotation rendering.
pub struct GitHubAnnotationResult {
    /// Rendered `::error`/`::warning`/`::notice` lines.
    pub lines: Vec<String>,
    /// Whether annotations were truncated due to cap.
    pub truncated: bool,
    /// Total number of annotations before capping.
    pub total_count: usize,
    /// Number of annotations actually rendered.
    pub rendered_count: usize,
}

/// Render annotations (file-level or inline findings) with capping.
pub fn render_annotations(
    highlights: &[Highlight],
    cfg: &CockpitConfig,
    sensor_blocking: &std::collections::BTreeMap<String, bool>,
) -> AnnotationRenderResult {
    let max = cfg.policy.max_annotations;
    let total_count = highlights.len();

    let mut sorted: Vec<&Highlight> = highlights.iter().collect();
    sorted.sort_by(|a, b| {
        annotation_sort_key(a, sensor_blocking).cmp(&annotation_sort_key(b, sensor_blocking))
    });

    let truncated = total_count > max;
    let rendered_count = total_count.min(max);

    let mut out = String::new();

    if sorted.is_empty() {
        out.push_str("_No annotations._\n");
    } else {
        for (i, h) in sorted.iter().take(max).enumerate() {
            let f = &h.finding;
            let loc = match &f.location {
                Some(l) => {
                    let mut s = String::new();
                    if let Some(p) = &l.path {
                        s.push_str(p);
                    }
                    if let Some(line) = l.line {
                        s.push_str(&format!(":{}", line));
                    }
                    if s.is_empty() { None } else { Some(s) }
                }
                None => None,
            };

            let loc_str = loc.map(|x| format!(" at `{}`", x)).unwrap_or_default();
            out.push_str(&format!(
                "{}. {} **{}**: `{}`{} — {}\n",
                i + 1,
                severity_badge(&f.severity),
                h.sensor_id,
                f.code,
                loc_str,
                f.message.replace('\n', " ")
            ));
        }

        if truncated {
            out.push_str(&format!(
                "\n_Showing {} of {} annotations (capped by `max_annotations`)._\n",
                rendered_count, total_count
            ));
        }
    }

    AnnotationRenderResult {
        content: out,
        truncated,
        total_count,
        rendered_count,
    }
}

/// Render GitHub Actions workflow command annotations from highlights.
pub fn render_github_annotations(
    highlights: &[Highlight],
    cfg: &CockpitConfig,
    sensor_blocking: &std::collections::BTreeMap<String, bool>,
) -> GitHubAnnotationResult {
    let max = cfg.policy.max_annotations;
    let total_count = highlights.len();

    let mut sorted: Vec<&Highlight> = highlights.iter().collect();
    sorted.sort_by(|a, b| {
        annotation_sort_key(a, sensor_blocking).cmp(&annotation_sort_key(b, sensor_blocking))
    });

    let truncated = total_count > max;
    let rendered_count = total_count.min(max);

    let mut lines = Vec::with_capacity(rendered_count);
    for h in sorted.iter().take(max) {
        let f = &h.finding;
        let level = gh_level(&f.severity);
        let title = gh_escape(&format!("[{}] {}", h.sensor_id, f.code));

        let mut params = Vec::new();
        if let Some(loc) = &f.location {
            if let Some(path) = &loc.path {
                params.push(format!("file={}", path));
            }
            if let Some(line) = loc.line {
                params.push(format!("line={}", line));
            }
            if let Some(col) = loc.col {
                params.push(format!("col={}", col));
            }
        }
        params.push(format!("title={}", title));

        let message = gh_escape(&f.message);
        lines.push(format!("::{} {}::{}", level, params.join(","), message));
    }

    GitHubAnnotationResult {
        lines,
        truncated,
        total_count,
        rendered_count,
    }
}

fn severity_badge(s: &Severity) -> &'static str {
    match s {
        Severity::Error => "❌",
        Severity::Warn => "⚠️",
        Severity::Info => "ℹ️",
    }
}

fn gh_escape(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}

fn gh_level(s: &Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warn => "warning",
        Severity::Info => "notice",
    }
}

fn annotation_sort_key<'a>(
    h: &'a Highlight,
    sensor_blocking: &std::collections::BTreeMap<String, bool>,
) -> (
    u8,
    u8,
    &'a str,
    Option<&'a str>,
    Option<u32>,
    &'a str,
    &'a str,
) {
    let blocking = sensor_blocking.get(&h.sensor_id).cloned().unwrap_or(false);
    (
        severity_rank(&h.finding.severity),
        if blocking { 0u8 } else { 1u8 },
        &h.sensor_id,
        h.finding.location.as_ref().and_then(|l| l.path.as_deref()),
        h.finding.location.as_ref().and_then(|l| l.line),
        &h.finding.code,
        &h.finding.message,
    )
}
