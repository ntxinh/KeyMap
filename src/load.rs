use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Binding, Source};
use crate::parsers::{custom, fish, ghostty, niri};

/// Filesystem locations the loader reads. `load_all` builds these from the
/// real config home; tests build a fixture root.
pub struct ConfigPaths {
    /// `~/.config/niri/config.kdl`
    pub niri_config: PathBuf,
    /// `~/.config/niri/dms` — DankMaterialShell writes niri `*.kdl` here.
    pub niri_dms_dir: PathBuf,
    /// `~/.config/ghostty/config`
    pub ghostty_config: PathBuf,
    /// `~/.config/fish` — `config.fish`, `conf.d/*.fish`, `functions/*.fish`
    /// are scanned.
    pub fish_dir: PathBuf,
    /// `~/.config/keymap/apps` — `*.toml` app files.
    pub custom_apps_dir: PathBuf,
}

/// Real `~/.config` paths (honors `XDG_CONFIG_HOME`).
pub fn load_all() -> Vec<Binding> {
    load_from(&real_paths())
}

fn real_paths() -> ConfigPaths {
    let home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    ConfigPaths {
        niri_config: home.join("niri/config.kdl"),
        niri_dms_dir: home.join("niri/dms"),
        ghostty_config: home.join("ghostty/config"),
        fish_dir: home.join("fish"),
        custom_apps_dir: home.join("keymap/apps"),
    }
}

/// Load every binding source. Missing files/dirs are skipped; unreadable or
/// malformed content yields whatever its parser recovered.
pub fn load_from(paths: &ConfigPaths) -> Vec<Binding> {
    let mut out = Vec::new();

    if let Some(text) = read(&paths.niri_config) {
        out.extend(niri::parse_niri_kdl(&text, Source::Niri, &paths.niri_config, false));
    }
    for file in glob(&paths.niri_dms_dir, "kdl") {
        if let Some(text) = read(&file) {
            out.extend(niri::parse_niri_kdl(&text, Source::Dms, &file, false));
        }
    }
    out.extend(niri::niri_defaults(Path::new("<niri defaults>")));

    if let Some(text) = read(&paths.ghostty_config) {
        out.extend(ghostty::parse_ghostty_lines(
            &text,
            Source::Ghostty,
            &paths.ghostty_config,
            false,
        ));
    }
    out.extend(ghostty::ghostty_defaults(Path::new("<ghostty defaults>")));

    let mut fish_files = vec![paths.fish_dir.join("config.fish")];
    fish_files.extend(glob(&paths.fish_dir.join("conf.d"), "fish"));
    fish_files.extend(glob(&paths.fish_dir.join("functions"), "fish"));
    for file in fish_files {
        if let Some(text) = read(&file) {
            out.extend(fish::parse_bind_statements(&text, &file));
        }
    }
    out.extend(fish::fish_defaults(Path::new("<fish defaults>")));

    for file in glob(&paths.custom_apps_dir, "toml") {
        if let Some(text) = read(&file) {
            out.extend(custom::parse_app_toml(&text, &file));
        }
    }

    out
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// `dir/*.ext`, sorted for deterministic ordering; empty on missing dir.
fn glob(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == ext))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::create_dir_all;

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        create_dir_all(dir).unwrap();
        let p = dir.join(name);
        fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn load_from_fixture_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let niri_config = write(
            &root.join("niri"),
            "config.kdl",
            "input { keyboard { xkb { layout \"us\"; } } }\nbinds {\nMod+T { spawn \"kitty\"; }\n}\n",
        );
        let dms_dir = root.join("niri/dms");
        write(&dms_dir, "dms.kdl", "binds {\nMod+D { spawn \"dms\"; }\n}\n");
        let ghostty_config = write(
            &root.join("ghostty"),
            "config",
            "keybind = ctrl+shift+k=scroll_page_up\n",
        );
        let fish_dir = root.join("fish");
        write(&fish_dir, "config.fish", "bind \\cx do-thing\n");
        write(
            &fish_dir.join("conf.d"),
            "extra.fish",
            "bind \\ce edit-config\n",
        );
        write(&fish_dir.join("functions"), "f.fish", "bind \\cr reload\n");
        let apps_dir = root.join("keymap/apps");
        write(
            &apps_dir,
            "mpd.toml",
            "name = \"mpd\"\n[[binding]]\ncombo = \"ctrl+m\"\naction = \"mpc toggle\"\n",
        );

        let paths = ConfigPaths {
            niri_config,
            niri_dms_dir: dms_dir,
            ghostty_config,
            fish_dir,
            custom_apps_dir: apps_dir,
        };
        let bindings = load_from(&paths);

        // Fixture bindings: 1 niri + 1 dms + 1 ghostty + 3 fish + 1 custom.
        let fixture: Vec<&Binding> = bindings
            .iter()
            .filter(|b| b.origin.starts_with(root))
            .collect();
        assert_eq!(fixture.len(), 7);

        for src in [
            Source::Niri,
            Source::Dms,
            Source::Fish,
            Source::Ghostty,
            Source::Custom("mpd".to_string()),
        ] {
            assert!(bindings.iter().any(|b| b.source == src), "missing {src:?}");
        }
        // Default snapshots also load.
        assert!(bindings.iter().any(|b| b.is_default));
    }

    #[test]
    fn missing_paths_yield_defaults_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let paths = ConfigPaths {
            niri_config: root.join("none/config.kdl"),
            niri_dms_dir: root.join("none/dms"),
            ghostty_config: root.join("none/config"),
            fish_dir: root.join("none/fish"),
            custom_apps_dir: root.join("none/apps"),
        };
        let bindings = load_from(&paths);
        assert!(!bindings.is_empty());
        assert!(bindings.iter().all(|b| b.is_default));
    }
}
