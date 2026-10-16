use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mods(pub u8);
impl Mods {
    pub const CTRL: Mods = Mods(1);
    pub const ALT: Mods = Mods(2);
    pub const SHIFT: Mods = Mods(4);
    pub const SUPER: Mods = Mods(8);
    pub const NONE: Mods = Mods(0);
    pub fn contains(self, m: Mods) -> bool {
        self.0 & m.0 != 0
    }
    pub fn union(self, m: Mods) -> Mods {
        Mods(self.0 | m.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub mods: Mods,
    pub key: String,                            /* normalized lowercase */
    pub sequence: Option<Box<KeyCombo>>,        /* fish multi-key only */
}

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Niri,
    Dms,
    Fish,
    Ghostty,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Context {
    Global,
    Terminal,
    Shell,
    App,
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub combo: KeyCombo,
    pub raw: String,
    pub action: String,
    pub description: Option<String>,
    pub source: Source,
    pub origin: PathBuf,
    pub is_default: bool,
    pub context: Context,
}

impl Binding {
    pub fn sort_key(&self) -> String {
        format!("{}|{}", self.action, self.raw)
    }
}

impl std::fmt::Display for KeyCombo {
    // Renders as e.g. "Ctrl+Shift+c"; sequences rendered space-separated ("a b").
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (m, name) in [
            (Mods::CTRL, "Ctrl"),
            (Mods::ALT, "Alt"),
            (Mods::SUPER, "Super"),
            (Mods::SHIFT, "Shift"),
        ] {
            if self.mods.contains(m) {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&display_key(&self.key))?;
        if let Some(next) = &self.sequence {
            write!(f, " {next}")?;
        }
        Ok(())
    }
}

fn display_key(k: &str) -> String {
    let named = match k {
        "slash" => Some("Slash"),
        "backslash" => Some("Backslash"),
        "enter" => Some("Enter"),
        "space" => Some("Space"),
        "tab" => Some("Tab"),
        "backspace" => Some("Backspace"),
        "delete" => Some("Delete"),
        "escape" => Some("Escape"),
        "insert" => Some("Insert"),
        "home" => Some("Home"),
        "end" => Some("End"),
        "page_up" => Some("PageUp"),
        "page_down" => Some("PageDown"),
        "arrow_up" => Some("Up"),
        "arrow_down" => Some("Down"),
        "arrow_left" => Some("Left"),
        "arrow_right" => Some("Right"),
        _ => None,
    };
    if let Some(n) = named {
        return n.to_string();
    }
    // Named keys capitalize ("f1" -> "F1"); single chars stay as-is ("c").
    if k.chars().count() > 1 {
        let mut c = k.chars();
        c.next().unwrap().to_uppercase().collect::<String>() + c.as_str()
    } else {
        k.to_string()
    }
}
