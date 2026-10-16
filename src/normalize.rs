use crate::model::{KeyCombo, Mods};

/// Modifier word (case-insensitive) -> flag. `mod_key` resolves the
/// generic "Mod" word (niri's configurable modifier).
fn mod_flag(word: &str, mod_key: &str) -> Option<Mods> {
    let m = match word.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Mods::CTRL,
        "alt" => Mods::ALT,
        "shift" => Mods::SHIFT,
        "super" | "cmd" | "meta" => Mods::SUPER,
        "mod" => match mod_key.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Mods::CTRL,
            "alt" => Mods::ALT,
            "shift" => Mods::SHIFT,
            _ => Mods::SUPER,
        },
        _ => return None,
    };
    Some(m)
}

/// Normalize a single key token: lowercase, canonical arrow/enter names.
fn norm_key(s: &str) -> Option<String> {
    let k = s.trim().to_ascii_lowercase();
    if k.is_empty() {
        return None;
    }
    Some(match k.as_str() {
        "up" => "arrow_up".into(),
        "down" => "arrow_down".into(),
        "left" => "arrow_left".into(),
        "right" => "arrow_right".into(),
        "return" => "enter".into(),
        "minus" => "-".into(),
        _ => k,
    })
}

fn combo(mods: Mods, key: &str) -> Option<KeyCombo> {
    norm_key(key).map(|key| KeyCombo {
        mods,
        key,
        sequence: None,
    })
}

/// niri: "Mod+Shift+Slash", `+`-separated, "Mod" resolves via `mod_key`.
pub fn parse_niri_combo(s: &str, mod_key: &str) -> Option<KeyCombo> {
    let mut mods = Mods::NONE;
    let mut parts = s.split('+').peekable();
    while let Some(p) = parts.next() {
        if parts.peek().is_none() {
            return combo(mods, p);
        }
        mods = mods.union(mod_flag(p, mod_key)?);
    }
    None
}

/// ghostty/custom: "ctrl+shift+x", `+`-separated, all but last are mods.
/// A trailing '+' token is a literal "+" key: "ctrl++" → Ctrl+"+".
pub fn parse_ghostty_combo(s: &str) -> Option<KeyCombo> {
    if s.ends_with('+') {
        // Last '+' is the key; an adjacent '+' before it is the separator.
        let mods_part = &s[..s.len() - 1];
        let mods_part = mods_part.strip_suffix('+').unwrap_or(mods_part);
        let mut mods = Mods::NONE;
        if !mods_part.is_empty() {
            for p in mods_part.split('+') {
                mods = mods.union(mod_flag(p, "super")?);
            }
        }
        return combo(mods, "+");
    }
    parse_niri_combo(s, "super")
}

/// Same syntax as ghostty.
pub fn parse_custom_combo(s: &str) -> Option<KeyCombo> {
    parse_ghostty_combo(s)
}

/// fish: "ctrl-c" — `-`-separated leading modifier words, rest is the key.
/// Bare keys ("enter", "backspace", "up") pass through.
pub fn parse_fish_key(s: &str) -> Option<KeyCombo> {
    let mut mods = Mods::NONE;
    let mut rest = s;
    while let Some((head, tail)) = rest.split_once('-') {
        match mod_flag(head, "") {
            Some(m) => {
                mods = mods.union(m);
                rest = tail;
            }
            None => break,
        }
    }
    combo(mods, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Mods;

    #[test]
    fn niri_mod_maps_to_super() {
        let c = parse_niri_combo("Mod+Shift+Slash", "Super").unwrap();
        assert!(c.mods.contains(Mods::SUPER));
        assert!(c.mods.contains(Mods::SHIFT));
        assert_eq!(c.key, "slash");
        assert_eq!(c.to_string(), "Super+Shift+Slash");
    }

    #[test]
    fn modifier_order_canonical() {
        assert_eq!(
            parse_ghostty_combo("shift+ctrl+x").unwrap(),
            parse_ghostty_combo("ctrl+shift+x").unwrap()
        );
    }

    #[test]
    fn ghostty_trailing_plus_is_literal_key() {
        let c = parse_ghostty_combo("ctrl++").unwrap();
        assert_eq!(c.key, "+");
        assert!(c.mods.contains(Mods::CTRL));
    }

    #[test]
    fn fish_key() {
        assert_eq!(parse_fish_key("ctrl-c").unwrap().key, "c");
        assert!(parse_fish_key("ctrl-c").unwrap().mods.contains(Mods::CTRL));
        assert_eq!(parse_fish_key("enter").unwrap().key, "enter");
    }
}
