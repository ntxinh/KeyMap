use std::path::Path;

use kdl::{KdlDocument, KdlNode, KdlValue};

use crate::model::{Binding, Context, Source};
use crate::normalize::parse_niri_combo;

/// niri `mod-key` setting (child of `input`, or of `input > keyboard` per the
/// brief); defaults to `"Super"`. Parse failures return the default.
pub fn read_mod_key(text: &str) -> String {
    parse_doc(text).map(|d| mod_key_of(&d)).unwrap_or_else(|| "Super".into())
}

fn mod_key_of(doc: &KdlDocument) -> String {
    let Some(input) = doc.get("input").and_then(|n| n.children()) else {
        return "Super".into();
    };
    fn from(d: &KdlDocument) -> Option<&str> {
        d.get("mod-key")
            .and_then(|n| n.get(0))
            .and_then(|v| v.as_string())
    }
    input
        .get("keyboard")
        .and_then(|n| n.children())
        .and_then(|kb| from(kb))
        .or_else(|| from(input))
        .unwrap_or("Super")
        .to_string()
}

/// Parse a niri `binds {}` block. Unparseable text or missing/unparseable
/// combos yield what parsed — never aborts. All binds are `Context::Global`.
pub fn parse_niri_kdl(text: &str, source: Source, origin: &Path, is_default: bool) -> Vec<Binding> {
    let Some(doc) = parse_doc(text) else {
        return Vec::new();
    };
    let mod_key = mod_key_of(&doc);
    let Some(binds) = doc.get("binds").and_then(|n| n.children()) else {
        return Vec::new();
    };
    binds
        .nodes()
        .iter()
        .filter_map(|n| bind_node(n, &mod_key, &source, origin, is_default))
        .collect()
}

/// Snapshot of niri's shipped `default-config.kdl` (niri has no dump
/// subcommand); used as the compiled-in defaults listing.
const NIRI_DEFAULTS: &str = include_str!("niri_defaults.txt");

/// niri's built-in default bindings, parsed from the embedded snapshot.
pub fn niri_defaults(origin: &Path) -> Vec<Binding> {
    parse_niri_kdl(NIRI_DEFAULTS, Source::Niri, origin, true)
}

fn bind_node(
    node: &KdlNode,
    mod_key: &str,
    source: &Source,
    origin: &Path,
    is_default: bool,
) -> Option<Binding> {
    let raw = node.name().value();
    let combo = parse_niri_combo(raw, mod_key)?;
    let action = node
        .children()
        .and_then(|c| c.nodes().first())
        .map(action_text)?;
    let description = node
        .get("hotkey-overlay-title")
        .and_then(|v| v.as_string())
        .map(str::to_owned);
    Some(Binding {
        combo,
        raw: raw.to_string(),
        action,
        description,
        source: source.clone(),
        origin: origin.to_path_buf(),
        is_default,
        context: Context::Global,
    })
}

/// `spawn "a" 1` → "spawn a 1"; argless verbs → just the verb.
fn action_text(node: &KdlNode) -> String {
    let mut s = node.name().value().to_string();
    for e in node.entries() {
        if e.name().is_none() {
            s.push(' ');
            s.push_str(&value_text(e.value()));
        }
    }
    s
}

fn value_text(v: &KdlValue) -> String {
    v.as_string()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
}

/// v2 parse with v1 fallback (niri configs are KDL1). Strict KDL1 still
/// requires `;`/newline after every node, so a failed parse is retried after
/// inserting missing terminators around `}`.
fn parse_doc(text: &str) -> Option<KdlDocument> {
    if let Ok(d) = text.parse::<KdlDocument>() {
        return Some(d);
    }
    KdlDocument::parse_v1(&fix_terminators(text)).ok()
}

/// Compact configs like `xkb { layout "us"; }` omit the terminator strict
/// KDL1 wants between `}` and a following `}`/node. Inserts `;` there,
/// skipping strings and comments. Text that was already valid is untouched
/// (only called after a failed parse).
fn fix_terminators(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len() + 16);
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                out.push(b'"');
                i += 1;
                while i < b.len() {
                    let c = b[i];
                    out.push(c);
                    i += 1;
                    if c == b'\\' && i < b.len() {
                        out.push(b[i]);
                        i += 1;
                    } else if c == b'"' {
                        break;
                    }
                }
            }
            b'r' if matches!(b.get(i + 1), Some(b'"') | Some(b'#')) => {
                // raw string r"..." / r#"..."#
                out.push(b'r');
                i += 1;
                let mut hs = 0;
                while b.get(i) == Some(&b'#') {
                    hs += 1;
                    out.push(b'#');
                    i += 1;
                }
                if b.get(i) == Some(&b'"') {
                    out.push(b'"');
                    i += 1;
                    while i < b.len() {
                        let c = b[i];
                        out.push(c);
                        i += 1;
                        if c == b'"' && b[i..].starts_with(&vec![b'#'; hs]) {
                            out.extend_from_slice(&b[i..i + hs]);
                            i += hs;
                            break;
                        }
                    }
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') || b.get(i + 1) == Some(&b'-') => {
                // // comment or /- slashdash: skip to newline
                while i < b.len() && b[i] != b'\n' {
                    out.push(b[i]);
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                // nested /* */ comment
                let mut depth = 0;
                while i < b.len() {
                    if b[i..].starts_with(b"/*") {
                        depth += 1;
                        out.extend_from_slice(b"/*");
                        i += 2;
                    } else if b[i..].starts_with(b"*/") {
                        depth -= 1;
                        out.extend_from_slice(b"*/");
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        out.push(b[i]);
                        i += 1;
                    }
                }
            }
            b'}' => {
                // terminate the node that just ended, if needed
                if prev_sig(&out).is_some_and(|c| !matches!(c, b';' | b'{' | b'\n' | b'\r')) {
                    out.push(b';');
                }
                out.push(b'}');
                i += 1;
                while matches!(b.get(i), Some(b' ') | Some(b'\t')) {
                    out.push(b[i]);
                    i += 1;
                }
                // terminate this node unless already followed by one
                if !matches!(b.get(i), None | Some(b';') | Some(b'\n') | Some(b'\r') | Some(b'/')) {
                    out.push(b';');
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// Last significant byte emitted (skipping trailing spaces/tabs).
fn prev_sig(out: &[u8]) -> Option<u8> {
    out.iter().rev().find(|&&c| c != b' ' && c != b'\t').copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Mods;

    const FIXTURE: &str = r#"
input { keyboard { xkb { layout "us"; } } }
binds {
    Mod+T hotkey-overlay-title="Open a Terminal: ghostty" { spawn "ghostty"; }
    Mod+Q repeat=false { close-window; }
    Super+Alt+L { spawn-sh "swaylock"; }
}
"#;

    #[test]
    fn parses_binds() {
        let b = parse_niri_kdl(FIXTURE, Source::Niri, Path::new("config.kdl"), false);
        assert_eq!(b.len(), 3);
        let term = &b[0];
        assert_eq!(term.action, "spawn ghostty");
        assert_eq!(term.description.as_deref(), Some("Open a Terminal: ghostty"));
        assert_eq!(b[1].action, "close-window");
        assert_eq!(b[2].action, "spawn-sh swaylock");
        assert!(b.iter().all(|x| x.context == Context::Global && !x.is_default));
    }

    const ALT_FIXTURE: &str = r#"
input {
    keyboard {
        xkb { layout "us" }
        mod-key "Alt"
    }
}
binds { Mod+Q { close-window; } }
"#;

    #[test]
    fn reads_mod_key() {
        assert_eq!(read_mod_key(FIXTURE), "Super");
        assert_eq!(read_mod_key(ALT_FIXTURE), "Alt");
        assert_eq!(read_mod_key("garbage {{{"), "Super");
        let b = parse_niri_kdl(ALT_FIXTURE, Source::Dms, Path::new("dms/x.kdl"), true);
        assert_eq!(b[0].combo.mods, Mods::ALT);
        assert!(b[0].is_default);
        assert_eq!(b[0].source, Source::Dms);
    }
}
