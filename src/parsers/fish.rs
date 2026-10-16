use std::borrow::Cow;
use std::path::Path;
use std::process::Command;

use crate::model::{Binding, Context, KeyCombo, Mods, Source};
use crate::normalize::parse_fish_key;

/// Snapshot of `fish -c bind` on this machine; used when the binary is
/// absent or errors.
const FISH_DEFAULTS: &str = include_str!("fish_defaults.txt");

/// Parse user `bind` statements from fish config text. Comments,
/// non-`bind` lines, `bind --erase`, and keyless `bind` calls are skipped.
/// All binds are `Source::Fish` / `Context::Shell`.
pub fn parse_bind_statements(text: &str, origin: &Path) -> Vec<Binding> {
    text.lines().filter_map(|l| bind_line(l, origin, false)).collect()
}

/// Parse `fish -c bind` output (`bind --preset ...` rows). Rows are
/// `is_default` per the `--preset` flag; origin is the pseudo-path
/// "<fish -c bind>".
pub fn parse_preset_lines(text: &str) -> Vec<Binding> {
    preset_lines(text, Path::new("<fish -c bind>"))
}

fn preset_lines(text: &str, origin: &Path) -> Vec<Binding> {
    text.lines().filter_map(|l| bind_line(l, origin, true)).collect()
}

/// `fish -c bind` output lines; empty on spawn failure, non-zero exit, or
/// invalid UTF-8.
pub fn list_bind_binary() -> Vec<String> {
    let Ok(out) = Command::new("fish").args(["-c", "bind"]).output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8(out.stdout)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Default bindings: live `fish -c bind` output, falling back to the
/// embedded snapshot.
pub fn fish_defaults(origin: &Path) -> Vec<Binding> {
    let lines = list_bind_binary();
    let text = if lines.is_empty() { FISH_DEFAULTS.to_string() } else { lines.join("\n") };
    preset_lines(&text, origin)
}

/// One `bind` line. Grammar: `bind [-M mode] [-k] [--preset|--user]
/// <key...> [action...]`. Past the first key token, more tokens join the
/// sequence only while key-shaped (single char, modifier- or
/// escape-prefixed) — fish reads SEQUENCE as one token, but multi-token
/// forms appear in the wild and command verbs ("up-line", "self-insert")
/// never look key-shaped.
fn bind_line(line: &str, origin: &Path, mut is_default: bool) -> Option<Binding> {
    let toks = tokens(line);
    if toks.first().map(String::as_str) != Some("bind") {
        return None;
    }
    let mut i = 1;
    while let Some(t) = toks.get(i) {
        match t.as_str() {
            "-e" | "--erase" => return None, // erases a bind; produces none
            "-M" | "--mode" | "-m" | "--sets-mode" => i += 2,
            "-k" | "--key" => i += 1,
            "--preset" => {
                is_default = true;
                i += 1;
            }
            "--user" => {
                is_default = false;
                i += 1;
            }
            "--" => {
                i += 1;
                break;
            }
            _ if t.starts_with("--mode=") || t.starts_with("--sets-mode=") => i += 1,
            _ if t.starts_with('-') && t.len() > 1 => i += 1,
            _ => break,
        }
    }
    let mut keys: Vec<(KeyCombo, &str)> = Vec::new();
    while let Some(t) = toks.get(i) {
        let Some(c) = parse_fish_key(&fish_unescape(t)) else { break };
        let shaped = c.mods != Mods::NONE || t.chars().count() == 1 || t.starts_with('\\');
        if !keys.is_empty() && !shaped {
            break;
        }
        keys.push((c, t));
        i += 1;
    }
    if keys.is_empty() {
        return None;
    }
    let raw = keys.iter().map(|(_, t)| *t).collect::<Vec<_>>().join(" ");
    let mut iter = keys.into_iter().map(|(c, _)| c);
    let mut combo = iter.next().unwrap();
    for next in iter {
        push_sequence(&mut combo, next);
    }
    Some(Binding {
        combo,
        raw,
        action: toks[i..].join(" "),
        description: None,
        source: Source::Fish,
        origin: origin.to_path_buf(),
        is_default,
        context: Context::Shell,
    })
}

fn push_sequence(head: &mut KeyCombo, next: KeyCombo) {
    match &mut head.sequence {
        Some(tail) => push_sequence(tail, next),
        None => head.sequence = Some(Box::new(next)),
    }
}

/// `\cx` / `\cX` → `ctrl-x`; a lone `\e` → `escape`. Other escapes keep
/// their raw shape so `\e\[A`-style terminal sequences stay literal keys.
fn fish_unescape(t: &str) -> Cow<'_, str> {
    if t == "\\e" {
        return Cow::Borrowed("escape");
    }
    if let Some(k) = t.strip_prefix("\\c").or_else(|| t.strip_prefix("\\C")) {
        if k.chars().count() == 1 {
            return Cow::Owned(format!("ctrl-{}", k.to_ascii_lowercase()));
        }
    }
    Cow::Borrowed(t)
}

/// Shell-ish split: whitespace-separated; single/double quotes group;
/// backslash escapes pass through verbatim (fish key syntax needs `\cX`).
/// `#` starting a token comments out the rest of the line.
fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut open = false;
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        if !open {
            match c {
                '#' => break,
                _ if c.is_whitespace() => continue,
                _ => open = true,
            }
        }
        match c {
            '\'' => {
                for q in it.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    cur.push(q);
                }
            }
            '"' => {
                while let Some(q) = it.next() {
                    match q {
                        '"' => break,
                        '\\' => {
                            cur.push('\\');
                            if let Some(e) = it.next() {
                                cur.push(e);
                            }
                        }
                        _ => cur.push(q),
                    }
                }
            }
            '\\' => {
                cur.push('\\');
                if let Some(e) = it.next() {
                    cur.push(e);
                }
            }
            _ if c.is_whitespace() => {
                out.push(std::mem::take(&mut cur));
                open = false;
            }
            _ => cur.push(c),
        }
    }
    if open {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Mods;

    #[test]
    fn parses_user_bind() {
        let b = parse_bind_statements(
            "bind ctrl-g 'git status'\nbind \\co overwrite-by-line\n",
            Path::new("conf.d/x.fish"),
        );
        assert_eq!(b[0].combo.key, "g");
        assert!(b[0].combo.mods.contains(Mods::CTRL));
        assert_eq!(b[0].action, "git status");
        assert_eq!(b[1].combo.key, "o");
        assert!(b[1].combo.mods.contains(Mods::CTRL));
        assert_eq!(b[1].action, "overwrite-by-line");
        assert!(!b[0].is_default && b[0].context == Context::Shell);
    }

    #[test]
    fn parses_preset_rows() {
        let b = parse_preset_lines(
            "bind --preset ctrl-c cancel-commandline\nbind --preset up up-line\n",
        );
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].action, "cancel-commandline");
        assert!(b[0].is_default);
        assert!(b[0].combo.mods.contains(Mods::CTRL));
        assert_eq!(b[1].combo.key, "arrow_up");
    }

    #[test]
    fn multi_key_sequence() {
        let b = parse_bind_statements(
            "bind ctrl-x b foo\nbind -M insert -k enter accept\n",
            Path::new("x.fish"),
        );
        let seq = b[0].combo.sequence.as_ref().unwrap();
        assert_eq!(seq.key, "b");
        assert_eq!(b[0].action, "foo");
        assert_eq!(b[1].combo.key, "enter");
        assert_eq!(b[1].action, "accept");
    }

    #[test]
    fn snapshot_parses() {
        let b = parse_preset_lines(FISH_DEFAULTS);
        assert!(b.len() >= 20);
        assert!(
            b.iter()
                .all(|x| x.is_default && x.context == Context::Shell && x.source == Source::Fish)
        );
        assert!(b
            .iter()
            .any(|x| x.combo.key == "c"
                && x.combo.mods.contains(Mods::CTRL)
                && x.action == "cancel-commandline"));
    }
}
