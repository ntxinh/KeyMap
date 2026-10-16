use std::path::Path;
use std::process::Command;

use crate::model::{Binding, Context, Source};
use crate::normalize::parse_ghostty_combo;

/// Snapshot of `ghostty +list-keybinds` on this machine; used when the
/// binary is absent or errors.
const GHOSTTY_DEFAULTS: &str = include_str!("ghostty_defaults.txt");

/// Parse `keybind = mods+key=action` lines (config file or
/// `+list-keybinds` output — same format). Lines without `=`, other
/// directives, or unparseable combos are skipped. All binds are
/// `Context::Terminal`.
pub fn parse_ghostty_lines(
    text: &str,
    source: Source,
    origin: &Path,
    is_default: bool,
) -> Vec<Binding> {
    text.lines().filter_map(|l| bind_line(l, &source, origin, is_default)).collect()
}

fn bind_line(line: &str, source: &Source, origin: &Path, is_default: bool) -> Option<Binding> {
    let (directive, value) = line.split_once('=')?;
    if directive.trim() != "keybind" {
        return None;
    }
    // rsplit: the combo itself may end in '=' ("ctrl+==" = Ctrl+'='), while
    // ghostty actions carry no '='.
    let (combo_raw, action) = value.rsplit_once('=')?;
    let action = action.trim();
    if action.is_empty() {
        return None;
    }
    let combo_raw = combo_raw.trim();
    let combo = parse_ghostty_combo(combo_raw)?;
    Some(Binding {
        combo,
        raw: combo_raw.to_string(),
        action: action.to_string(),
        description: None,
        source: source.clone(),
        origin: origin.to_path_buf(),
        is_default,
        context: Context::Terminal,
    })
}

/// `ghostty +list-keybinds` output lines; empty on spawn failure, non-zero
/// exit, or invalid UTF-8.
pub fn list_keybinds_binary() -> Vec<String> {
    let Ok(out) = Command::new("ghostty").arg("+list-keybinds").output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8(out.stdout)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Default bindings: live `+list-keybinds` output, falling back to the
/// embedded snapshot.
pub fn ghostty_defaults(origin: &Path) -> Vec<Binding> {
    let lines = list_keybinds_binary();
    let text = if lines.is_empty() { GHOSTTY_DEFAULTS.to_string() } else { lines.join("\n") };
    parse_ghostty_lines(&text, Source::Ghostty, origin, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Mods;

    #[test]
    fn parses_keybind_lines() {
        let text = "keybind = shift+alt+arrow_down=goto_split:down\nkeybind = ctrl+shift+c=copy_to_clipboard:mixed\n";
        let b = parse_ghostty_lines(text, Source::Ghostty, Path::new("config"), false);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].action, "goto_split:down");
        assert!(b[0].combo.mods.contains(Mods::SHIFT.union(Mods::ALT)));
        assert_eq!(b[0].combo.key, "arrow_down");
    }

    #[test]
    fn equals_and_plus_keys() {
        let text = "keybind = ctrl+==increase_font_size:1\nkeybind = ctrl++=increase_font_size:1\n";
        let b = parse_ghostty_lines(text, Source::Ghostty, Path::new("defaults"), true);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].action, "increase_font_size:1");
        assert_eq!(b[0].combo.key, "=");
        assert!(b[0].combo.mods.contains(Mods::CTRL));
        assert_eq!(b[1].combo.key, "+");
        assert!(b[1].combo.mods.contains(Mods::CTRL));
    }

    #[test]
    fn snapshot_parses() {
        let b = parse_ghostty_lines(GHOSTTY_DEFAULTS, Source::Ghostty, Path::new("defaults"), true);
        assert!(b.len() > 50);
        assert!(b.iter().all(|x| x.is_default && x.context == Context::Terminal));
        // The two previously-dropped literal-key defaults now parse.
        assert!(b.iter().any(|x| x.combo.key == "=" && x.action == "increase_font_size:1"));
        assert!(b.iter().any(|x| x.combo.key == "+" && x.action == "increase_font_size:1"));
    }
}
