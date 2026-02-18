# cockpitctl-render

Deterministic renderers for cockpit markdown comments and CI annotations.

## Scope
- Renders stable PR comment markdown with cockpit markers.
- Applies annotation budgets from policy (`max_annotations`).
- Emits deterministic GitHub Actions workflow annotations.
- Renders buildfix, trend, and policy-signature sections when present.

## Key exports
- `render_comment`
- `render_annotations`, `render_annotations_section`
- `render_github_annotations`
- `append_comment_sections`
