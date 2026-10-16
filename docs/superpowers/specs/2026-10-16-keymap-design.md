# KeyMap — Shortcut Explorer Design

2026-10-16 · Single Rust binary, GTK4/Relm4 UI, read-only keybinding explorer + cheat-sheet generator for Fedora 44 / Wayland / Niri.

## 1. Architecture

Single crate, single binary `keymap`. Library structure inside the crate:

```
src/
  main.rs          # relm4 app entry
  model.rs         # Binding, KeyCombo, Source, Context, Mods
  normalize.rs     # per-source combo syntax → KeyCombo
  parsers/
    niri.rs        # kdl crate, binds{} block; also niri/dms/*.kdl → Source::Dms
    ghostty.rs     # keybind= lines + `ghostty +list-keybinds` for defaults
    fish.rs        # user bind statements + `fish -c bind` for defaults
    custom.rs      # ~/.config/keymap/apps/*.toml
  conflict.rs      # duplicate + fish-prefix conflicts
  report.rs        # markdown cheatsheet
  watch.rs         # notify-debouncer-full → relm4 channel → re-parse
  ui/              # relm4 components
```

Data flows one way: parsers → `Vec<Binding>` → (search/filter + conflict pass) → UI. Report reads the same `Vec<Binding>`.

## 2. Data Model

    context: Context,              // Global | WindowFocused | Terminal | Shell | App
}
```

Context assignment: derived from source (Niri/DMS → Global, Ghostty → Terminal, Fish → Shell, Custom → App); Niri binds with `allow-when-locked` stay Global. UI grouping uses Context.

Normalization rules (`normalize.rs`, single place for all syntax mapping):

- Niri `Mod` → the value of niri's `mod-key` setting if configured, else `Super`.
- Ghostty `ctrl+shift+c`, niri `Mod+Shift+Slash`, fish `ctrl-c` all normalize to the same `KeyCombo`.
- Modifier order canonicalized (Ctrl, Alt, Shift, Super) so `Shift+Ctrl+X` == `Ctrl+Shift+X`.

## 3. Parsers

| Source | Parse | Defaults |
|---|---|---|
| Niri | `kdl` crate on `~/.config/niri/config.kdl`, `binds {}` block. `{ spawn "x"; }` → `spawn x`; `hotkey-overlay-title` prop → description. `mod-key` setting read for Mod normalization. | `~/.config/niri/dms/*.kdl` includes → `Source::Dms`. Niri's compiled-in defaults embedded as data. |
| Ghostty | `keybind = mods+key=action` lines in `~/.config/ghostty/config`. | `ghostty +list-keybinds` subprocess at load (defaults only — verified it does not merge user config). Fallback: embedded snapshot if binary absent. |
| Fish | `bind` statements in `config.fish`, `conf.d/*.fish`, `functions/*.fish`. | `fish -c bind` (`--preset` rows = effective defaults). Fallback: embedded snapshot. |
| Custom | `~/.config/keymap/apps/*.toml`: `[[binding]] combo="ctrl+x" action="…" [description="…"]`. | n/a |

Rules: real format → real crate (`kdl`, `toml`+`serde`); ghostty/fish lines are `split`-level. A parser that fails returns what it parsed + warning, never aborts the load.

## 4. Conflict Detection

1. **Exact duplicate**: identical normalized `KeyCombo` claimed by ≥2 entries → conflict group.
2. **Fish prefix**: a fish multi-key sequence whose first key collides with a global binding → flagged. Deeper sequence/shadowing analysis is out of scope (noise).

Informational only — never edits configs. Output: `HashMap<KeyCombo, Vec<Binding>>` len>1 plus prefix flags.

## 5. UI (Relm4)

One `AdwWindow`:

- `GtkSearchEntry` at top — case-insensitive substring over combo + action + description.
- Center list: rows `combo | action | description | source-badge`; grouped by Context; `is_default` rows dimmed + "hide defaults" toggle.
- Source filter chips (Niri / DMS / Ghostty / Fish / Custom).
- "Conflicts" button → dialog listing conflicting combos and their claimants.
- "Generate Cheat Sheet" (file save) + "Copy to Clipboard" (`gdk::Display::clipboard()`).

## 6. File Watching

`notify-debouncer-full` on `~/.config/{niri,fish,ghostty,DankMaterialShell,keymap}`; debounced event → relm4 `Sender` → full re-parse → refresh. Re-parse-all on any change (cheap at this data size; no incremental merging).

## 7. Cheat Sheet Report

Markdown, grouped Source → Context → rows (`| keys | action | description |`). Written to a user-chosen path via save dialog; clipboard variant copies the same text.

## 8. Testing

- Parser fixtures: realistic `.kdl`, ghostty `keybind=` lines, fish `bind` lines, custom TOML — assert parsed `Binding`s.
- `normalize.rs`: table test across all source syntaxes incl. modifier ordering + niri `Mod`.
- `conflict.rs`: unit tests for exact + prefix cases.
- Smoke: run the app on the target machine, verify real bindings appear, conflicts found, cheat sheet generated.
- No UI test harness.

## 9. Non-Goals (from PRD)

Read-only (no config editing), no other DEs/WMs, no cloud sync, no remapper, no plugin system, no database.
