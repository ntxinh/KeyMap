use std::borrow::Cow;
use std::path::Path;
use std::process::Command;

use crate::model::{Binding, Context, KeyCombo, Source};
use crate::normalize::parse_fish_key;

/// Snapshot of `fish -c bind` on this machine; used when the binary is
/// absent or errors.
const FISH_DEFAULTS: &str = include_str!("fish_defaults.txt");

/// Parse user `bind` statements from fish config text. Comments,
/// non-`bind` lines, `bind --erase`, and keyless `bind` calls are skipped.
/// All binds are `Source::Fish` / `Context::Shell`, `is_default` derived
/// from the line's own flags (normally false).
pub fn parse_bind_statements(text: &str, origin: &Path) -> Vec<Binding> {
    text.lines().filter_map(|l| bind_line(l, origin)).collect()
}

/// Parse `fish -c bind` output. `fish -c` sources user config, so output
/// mixes `bind --preset ...` defaults with bare `bind key action` user
/// rows — `is_default` is derived from each row's own flags.
#[cfg(test)] // used only by parser tests
pub fn parse_preset_lines(text: &str) -> Vec<Binding> {
    preset_rows(text, Path::new("<fish -c bind>"))
}

/// `--preset` rows of `fish -c bind` output. `fish -c` sources user
/// config, so bare rows are user binds — `load_from` already parses those
/// from the fish files; keeping them here would double-load every user
/// bind (duplicated rows + phantom Exact conflicts).
fn preset_rows(text: &str, origin: &Path) -> Vec<Binding> {
    text.lines()
        .filter_map(|l| bind_line(l, origin))
        .filter(|b| b.is_default)
        .collect()
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

/// Default bindings: `--preset` rows of live `fish -c bind` output,
/// falling back to the embedded snapshot. User rows in the live output
/// are skipped — file parsing owns them.
pub fn fish_defaults(origin: &Path) -> Vec<Binding> {
    let lines = list_bind_binary();
    let text = if lines.is_empty() {
        FISH_DEFAULTS.to_string()
    } else {
        lines.join("\n")
    };
    preset_rows(&text, origin)
}

/// One `bind` line. Grammar: `bind [-M mode] [-k] [--preset|--user]
/// <key> [action...]` — fish reads SEQUENCE as a single token (commas or
/// concatenated escapes chain into `combo.sequence`), so everything after
/// the key token is the action. `bind a b cmd` means seq `a`, action
/// `b cmd`.
fn bind_line(line: &str, origin: &Path) -> Option<Binding> {
    let toks = tokens(line);
    if toks.first().map(String::as_str) != Some("bind") {
        return None;
    }
    let mut is_default = false;
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
    let key_tok = toks.get(i)?;
    let combo = fish_combo(key_tok)?;
    Some(Binding {
        combo,
        raw: key_tok.clone(),
        action: toks[i + 1..].join(" "),
        description: None,
        source: Source::Fish,
        origin: origin.to_path_buf(),
        is_default,
        context: Context::Shell,
    })
}

/// One SEQUENCE token → chained combo. Comma-joined (`ctrl-x,ctrl-y`) and
/// concatenated-escape (`\cx\cy`) forms split into per-key segments, each
/// normalized via `parse_fish_key`; a bare single key yields no sequence.
fn fish_combo(token: &str) -> Option<KeyCombo> {
    let combos: Option<Vec<KeyCombo>> = token
        .split(',')
        .filter(|p| !p.is_empty())
        .flat_map(key_segments)
        .map(|s| parse_fish_key(&fish_unescape(s)))
        .collect();
    let mut it = combos?.into_iter();
    let mut head = it.next()?;
    for next in it {
        push_sequence(&mut head, next);
    }
    Some(head)
}

/// Split one comma-separated part at each `\` after position 0:
/// `\cx\cy` → ["\cx", "\cy"], `\e\[A` → ["\e", "\[A"].
fn key_segments(part: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (idx, ch) in part.char_indices().skip(1) {
        if ch == '\\' {
            out.push(&part[start..idx]);
            start = idx;
        }
    }
    out.push(&part[start..]);
    out
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
    fn bind_output_keeps_only_preset_rows() {
        // `fish -c bind` sources user config: bare rows are user binds
        // already covered by file parsing — defaults keep only `--preset`.
        let b = parse_preset_lines(
            "bind ctrl-g 'git status'\nbind --preset ctrl-c cancel-commandline\n",
        );
        assert_eq!(b.len(), 1);
        assert!(b[0].is_default);
        assert_eq!(b[0].action, "cancel-commandline");
    }

    #[test]
    fn single_token_is_key_rest_is_action() {
        // fish SEQUENCE is one token: `bind a b cmd` → seq `a`, action `b cmd`.
        let b = parse_bind_statements("bind a b cmd\n", Path::new("x.fish"));
        assert_eq!(b[0].combo.key, "a");
        assert!(b[0].combo.sequence.is_none());
        assert_eq!(b[0].action, "b cmd");
    }

    #[test]
    fn comma_sequence_chains() {
        let b = parse_bind_statements("bind ctrl-x,ctrl-y foo\n", Path::new("x.fish"));
        let seq = b[0].combo.sequence.as_ref().unwrap();
        assert_eq!(b[0].combo.key, "x");
        assert!(b[0].combo.mods.contains(Mods::CTRL));
        assert_eq!(seq.key, "y");
        assert!(seq.mods.contains(Mods::CTRL));
        assert_eq!(b[0].action, "foo");
    }

    #[test]
    fn concatenated_escapes_chain() {
        let b = parse_bind_statements("bind \\cx\\cy foo\n", Path::new("x.fish"));
        let seq = b[0].combo.sequence.as_ref().unwrap();
        assert_eq!(b[0].combo.key, "x");
        assert!(b[0].combo.mods.contains(Mods::CTRL));
        assert_eq!(seq.key, "y");
        assert!(seq.mods.contains(Mods::CTRL));
        assert_eq!(b[0].action, "foo");
    }

    #[test]
    fn flag_options_skip() {
        let b = parse_bind_statements("bind -M insert -k enter accept\n", Path::new("x.fish"));
        assert_eq!(b[0].combo.key, "enter");
        assert_eq!(b[0].action, "accept");
    }

    #[test]
    fn snapshot_parses() {
        let b = parse_preset_lines(FISH_DEFAULTS);
        assert!(b.len() >= 20);
        assert!(b
            .iter()
            .all(|x| x.is_default && x.context == Context::Shell && x.source == Source::Fish));
        assert!(b.iter().any(|x| x.combo.key == "c"
            && x.combo.mods.contains(Mods::CTRL)
            && x.action == "cancel-commandline"));
    }
}
