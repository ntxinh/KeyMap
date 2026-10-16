use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{Binding, Context, Source};

/// Renders all bindings as a grouped markdown cheat sheet. Source sections
/// are ordered Niri, Dms, Ghostty, Fish, then Custom sources alphabetically;
/// bindings are subgrouped by context under `###` headings inside each source.
pub fn render_markdown(bindings: &[Binding]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# KeyMap Cheat Sheet\n");
    let _ = writeln!(out, "_Generated {}_\n", today());

    let mut by_source: BTreeMap<(u8, String), (&str, Vec<&Binding>)> = BTreeMap::new();
    for b in bindings {
        by_source
            .entry(source_key(&b.source))
            .or_insert_with(|| (source_label(&b.source), Vec::new()))
            .1
            .push(b);
    }

    for ((_, _), (label, mut group)) in by_source {
        let _ = writeln!(out, "## {label}\n");
        group.sort_by_key(|b| (context_rank(&b.context), b.sort_key()));

        let mut ctx: Option<&Context> = None;
        for b in group {
            if ctx != Some(&b.context) {
                ctx = Some(&b.context);
                let _ = writeln!(out, "### {}\n", context_label(&b.context));
                let _ = writeln!(out, "| Keys | Action | Description |");
                let _ = writeln!(out, "|---|---|---|");
            }
            let desc = b.description.as_deref().unwrap_or("—");
            let _ = writeln!(
                out,
                "| {} | {} | {} |",
                esc(&b.combo.to_string()),
                esc(&b.action),
                esc(desc)
            );
        }
        out.push('\n');
    }
    out
}

fn esc(s: &str) -> String {
    s.replace('|', "\\|")
}

fn source_key(s: &Source) -> (u8, String) {
    match s {
        Source::Niri => (0, String::new()),
        Source::Dms => (1, String::new()),
        Source::Ghostty => (2, String::new()),
        Source::Fish => (3, String::new()),
        Source::Custom(name) => (4, name.to_lowercase()),
    }
}

fn source_label(s: &Source) -> &str {
    match s {
        Source::Niri => "Niri",
        Source::Dms => "Dms",
        Source::Ghostty => "Ghostty",
        Source::Fish => "Fish",
        Source::Custom(name) => name,
    }
}

fn context_rank(c: &Context) -> u8 {
    match c {
        Context::Global => 0,
        Context::Terminal => 1,
        Context::Shell => 2,
        Context::App => 3,
    }
}

fn context_label(c: &Context) -> &'static str {
    match c {
        Context::Global => "Global",
        Context::Terminal => "Terminal",
        Context::Shell => "Shell",
        Context::App => "App",
    }
}

/// UTC date without a date crate (Hinnant's civil-from-days).
fn today() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86400) as i64)
        .unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KeyCombo;
    use crate::model::Mods;
    use std::path::PathBuf;

    fn binding(source: Source, mods: Mods, key: &str, action: &str, ctx: Context) -> Binding {
        Binding {
            combo: KeyCombo { mods, key: key.into(), sequence: None },
            raw: String::new(),
            action: action.into(),
            description: None,
            source,
            origin: PathBuf::new(),
            is_default: false,
            context: ctx,
        }
    }

    #[test]
    fn renders_source_sections_and_rows() {
        let mut niri = binding(
            Source::Niri,
            Mods::SUPER.union(Mods::SHIFT),
            "slash",
            "show-hotkey-overlay",
            Context::Global,
        );
        niri.description = Some("Show hotkeys".into());
        let dms = binding(Source::Dms, Mods::ALT, "x", "spawn dms", Context::Global);
        let fish = binding(Source::Fish, Mods::NONE, "a", "fish cmd", Context::Shell);
        let out = render_markdown(&[fish, dms, niri]);

        assert!(out.starts_with("# KeyMap Cheat Sheet"));
        assert!(out.contains("| Keys | Action | Description |"));
        assert!(out.contains("| Super+Shift+Slash | show-hotkey-overlay | Show hotkeys |"));
        assert!(out.contains("### Global"));
        assert!(out.contains("### Shell"));

        // Fixed source order: Niri before Dms before Fish.
        let (n, d, f) = (out.find("## Niri"), out.find("## Dms"), out.find("## Fish"));
        assert!(n < d && d < f, "{out}");
    }

    #[test]
    fn custom_sources_sorted_alphabetically_last() {
        let zeta = binding(Source::Custom("Zeta".into()), Mods::NONE, "z", "z", Context::App);
        let alpha = binding(Source::Custom("alpha".into()), Mods::NONE, "a", "a", Context::App);
        let ghostty = binding(Source::Ghostty, Mods::CTRL, "c", "copy", Context::Terminal);
        let out = render_markdown(&[zeta, alpha, ghostty]);

        let (g, a, z) = (
            out.find("## Ghostty"),
            out.find("## alpha"),
            out.find("## Zeta"),
        );
        assert!(g < a && a < z, "{out}");
    }

    #[test]
    fn escapes_pipes_in_cells() {
        let b = binding(
            Source::Fish,
            Mods::NONE,
            "p",
            "spawn-sh \"a|b\"",
            Context::Shell,
        );
        let out = render_markdown(&[b]);
        assert!(out.contains("a\\|b"), "{out}");
        assert!(!out.contains("| spawn-sh \"a|b\" |"), "{out}");
    }

    #[test]
    fn missing_description_uses_placeholder() {
        let b = binding(Source::Niri, Mods::SUPER, "t", "spawn term", Context::Global);
        let out = render_markdown(&[b]);
        assert!(out.contains("| Super+t | spawn term | — |"), "{out}");
    }
}
