# KeyMap

A GTK4/libadwaita (relm4) viewer for every keyboard shortcut configured on a
niri/Wayland desktop. It lists, searches, and filters key bindings, flags
conflicts between them, and can export the whole set as a Markdown cheat
sheet.

KeyMap is **read-only**: it parses your config files and shows the results —
it never writes to any of them.

## Run

```sh
cargo run
```

Requires a Rust toolchain plus the GTK4 and libadwaita development packages.
`cargo test` runs the parser/report test suite.

## Sources

KeyMap reads bindings from these locations under `$XDG_CONFIG_HOME` (or
`~/.config`):

| Source | Path |
|---|---|
| niri | `niri/config.kdl` (`binds` block; `Mod` resolves from the `mod-key` setting under `input`, default Super) |
| DankMaterialShell | `niri/dms/*.kdl` |
| ghostty | `ghostty/config` (`keybind =` lines) |
| fish | `fish/config.fish`, `fish/conf.d/*.fish`, `fish/functions/*.fish` (`bind` statements) |
| custom apps | `keymap/apps/*.toml` (see below) |

Config directories are watched (500 ms debounce) and the list reloads
automatically when they change.

### Defaults vs. user bindings

Niri, ghostty, and fish also contribute a snapshot of their **default**
bindings (bundled in `src/parsers/*_defaults.txt`), shown in the list
alongside your own. Default rows are dimmed; the *Hide defaults* toggle
removes them entirely, so user bindings vs. defaults is always visible
at a glance.

Modifier names are normalized across sources: `ctrl`/`control`, `alt`,
`meta` (Alt, in fish/terminal vocab), `shift`, `super`/`cmd`, and `mod`
(niri's configurable `mod-key`, defaulting to Super) are all accepted
and display in canonical `Ctrl+Alt+Shift+Super` order.

## Custom app bindings

Apps whose shortcuts KeyMap can't parse natively can be declared as TOML
files in `~/.config/keymap/apps/`. One file per app:

```toml
name = "mpd"          # optional; defaults to the file stem

[[binding]]
combo = "ctrl+m"
action = "mpc toggle"
description = "Play/pause"   # optional
```

Combo syntax: `+`-separated, last token is the key, earlier tokens are
modifiers (case-insensitive; see the alias list under Sources). A
trailing `+` is a literal `+` key (`"ctrl++"` = Ctrl + `+`). Entries
with a malformed combo or missing `combo`/`action` are skipped
individually, not the file. Custom bindings appear under the app's name
in the `App` context.

## UI

- Search box filters by combo, action, or description.
- Toggle chips per source (Niri, Dms, Ghostty, Fish, Custom).
- *Conflicts* shows a dialog listing exact conflicts and fish prefix
  bindings that shadow each other.
- *Generate Cheat Sheet* writes a grouped Markdown file; *Copy* puts the
  same Markdown on the clipboard.

Screenshots live under `docs/` (to be added).
