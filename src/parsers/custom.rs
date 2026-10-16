use std::path::Path;

use serde::Deserialize;

use crate::model::{Binding, Context, Source};
use crate::normalize::parse_custom_combo;

#[derive(Deserialize)]
struct AppFile {
    /// Missing `name` falls back to the file stem.
    name: Option<String>,
    #[serde(default)]
    binding: Vec<AppBinding>,
}

#[derive(Deserialize)]
struct AppBinding {
    /* `toml::Value` (not Option<String>) so a missing *or* wrong-typed
       field skips this one entry instead of failing the whole file. */
    combo: Option<toml::Value>,
    action: Option<toml::Value>,
    description: Option<toml::Value>,
}

/// Parse a `~/.config/keymap/apps/*.toml` file. Malformed input yields an
/// empty vec (the loader surfaces the warning); bindings whose combo fails
/// `parse_custom_combo` are skipped individually. All binds are
/// `Context::App`, non-default, `Source::Custom(name)`.
pub fn parse_app_toml(text: &str, origin: &Path) -> Vec<Binding> {
    let Ok(file) = toml::from_str::<AppFile>(text) else {
        return Vec::new();
    };
    let name = file.name.unwrap_or_else(|| {
        origin
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let source = Source::Custom(name);
    file.binding
        .into_iter()
        .filter_map(|b| {
            let combo_raw = b.combo.as_ref()?.as_str()?;
            let action = b.action.as_ref()?.as_str()?;
            Some(Binding {
                combo: parse_custom_combo(combo_raw)?,
                raw: combo_raw.to_string(),
                action: action.to_string(),
                description: b.description.and_then(|v| v.as_str().map(str::to_string)),
                source: source.clone(),
                origin: origin.to_path_buf(),
                is_default: false,
                context: Context::App,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Mods;
    use std::path::PathBuf;

    #[test]
    fn parses_app_bindings() {
        let text = r#"
name = "Firefox"
[[binding]]
combo = "ctrl+shift+p"
action = "Private window"
description = "Opens private browsing"
"#;
        let origin = PathBuf::from("/home/u/.config/keymap/apps/firefox.toml");
        let bs = parse_app_toml(text, &origin);
        assert_eq!(bs.len(), 1);
        let b = &bs[0];
        assert_eq!(b.source, Source::Custom("Firefox".to_string()));
        assert_eq!(b.combo, parse_custom_combo("ctrl+shift+p").unwrap());
        assert!(b.combo.mods.contains(Mods::CTRL.union(Mods::SHIFT)));
        assert_eq!(b.context, Context::App);
        assert!(!b.is_default);
        assert_eq!(b.action, "Private window");
        assert_eq!(b.description.as_deref(), Some("Opens private browsing"));
        assert_eq!(b.origin, origin);
    }

    #[test]
    fn missing_name_uses_file_stem() {
        let text = "[[binding]]\ncombo = \"ctrl+n\"\naction = \"New\"\n";
        let bs = parse_app_toml(text, Path::new("/cfg/apps/kitty.toml"));
        assert_eq!(bs[0].source, Source::Custom("kitty".to_string()));
    }

    #[test]
    fn bad_combo_skips_entry_not_file() {
        let text = r#"
name = "App"
[[binding]]
combo = "ctrl+x+y"
action = "Bad"
[[binding]]
combo = "alt+f4"
action = "Quit"
"#;
        let bs = parse_app_toml(text, Path::new("app.toml"));
        assert_eq!(bs.len(), 1);
        assert_eq!(bs[0].action, "Quit");
    }

    #[test]
    fn bad_fields_skip_entry_not_file() {
        /* Missing `action`, wrong-typed `combo`, and wrong-typed
           `description` each kill only their own entry. */
        let text = r#"
name = "App"
[[binding]]
combo = "ctrl+a"
[[binding]]
combo = 5
action = "Bad"
[[binding]]
combo = "alt+f4"
action = "Quit"
description = 7
"#;
        let bs = parse_app_toml(text, Path::new("app.toml"));
        assert_eq!(bs.len(), 1);
        assert_eq!(bs[0].action, "Quit");
        assert_eq!(bs[0].description, None);
    }

    #[test]
    fn malformed_file_returns_empty() {
        assert!(parse_app_toml("this is = = not toml [[[", Path::new("x.toml")).is_empty());
        assert!(parse_app_toml("", Path::new("x.toml")).is_empty());
    }
}
