use std::path::PathBuf;
use std::time::Duration;

use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, Debouncer, RecommendedCache};

/// Emitted on the app channel when any watched config dir changes.
#[derive(Debug, Clone, Copy)]
pub enum WatchMsg {
    Reload,
}

/// Watch `~/.config/{niri,fish,ghostty,keymap}`
/// recursively with a 500ms debounce; any burst emits `WatchMsg::Reload`.
/// Returns the debouncer — the caller MUST hold it for the watch to live.
/// (Brief asked for `RecommendedWatcher`; the `Debouncer` owns it and can't
/// be split apart — it is what must be kept alive.)
///
/// Directories that don't exist are skipped: notify errors watching them,
/// and a missing dir gains files only via its parent's events anyway —
/// re-spawn the watcher after creating one (Task 9 concern).
pub fn spawn_watcher(
    send: relm4::Sender<WatchMsg>,
) -> notify_debouncer_full::notify::Result<Debouncer<RecommendedWatcher, RecommendedCache>> {
    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        None,
        move |result: notify_debouncer_full::DebounceEventResult| {
            if result.is_ok() {
                send.emit(WatchMsg::Reload);
            }
        },
    )?;

    let home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    for dir in ["niri", "fish", "ghostty", "keymap"] {
        let path = home.join(dir);
        if path.is_dir() {
            debouncer.watch(&path, RecursiveMode::Recursive)?;
        }
    }
    Ok(debouncer)
}
