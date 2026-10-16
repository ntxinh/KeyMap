use std::collections::HashSet;

use relm4::adw;
use relm4::adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::gtk;
use relm4::prelude::*;

use crate::conflict::{find_conflicts, Kind};
use crate::load::load_all;
use crate::model::{Binding, Context, Source};
use crate::report::render_markdown;
use crate::watch::spawn_watcher;

/// The four fixed sources plus one bucket for all custom app sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chip {
    Niri,
    Dms,
    Ghostty,
    Fish,
    Custom,
}

fn chip_of(source: &Source) -> Chip {
    match source {
        Source::Niri => Chip::Niri,
        Source::Dms => Chip::Dms,
        Source::Ghostty => Chip::Ghostty,
        Source::Fish => Chip::Fish,
        Source::Custom(_) => Chip::Custom,
    }
}

/// List item: a context group header or one binding row.
#[derive(Debug)]
pub enum Row {
    Header(&'static str),
    Entry {
        combo: String,
        action: String,
        desc: String,
        badge: String,
        dim: bool,
    },
}

#[relm4::factory(pub)]
impl FactoryComponent for Row {
    type Init = Row;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        gtk::ListBoxRow {
            #[watch]
            set_selectable: matches!(self, Row::Entry { .. }),
            #[watch]
            set_activatable: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,

                gtk::Label {
                    #[watch]
                    set_visible: matches!(self, Row::Header(_)),
                    #[watch]
                    set_label: match self {
                        Row::Header(text) => text,
                        _ => "",
                    },
                    add_css_class: "heading",
                    set_halign: gtk::Align::Start,
                    set_margin_top: 12,
                    set_margin_bottom: 4,
                    set_margin_start: 6,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 16,
                    set_margin_all: 6,
                    #[watch]
                    set_visible: matches!(self, Row::Entry { .. }),
                    #[watch]
                    set_opacity: match self {
                        Row::Entry { dim: true, .. } => 0.55,
                        _ => 1.0,
                    },

                    gtk::Label {
                        #[watch]
                        set_label: match self {
                            Row::Entry { combo, .. } => combo.as_str(),
                            _ => "",
                        },
                        set_width_request: 180,
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_selectable: true,
                    },

                    gtk::Label {
                        #[watch]
                        set_label: match self {
                            Row::Entry { action, .. } => action.as_str(),
                            _ => "",
                        },
                        set_width_request: 260,
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_selectable: true,
                    },

                    gtk::Label {
                        #[watch]
                        set_label: match self {
                            Row::Entry { desc, .. } => desc.as_str(),
                            _ => "",
                        },
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_selectable: true,
                    },

                    gtk::Label {
                        #[watch]
                        set_label: match self {
                            Row::Entry { badge, .. } => badge.as_str(),
                            _ => "",
                        },
                        add_css_class: "dim-label",
                        set_halign: gtk::Align::End,
                    },
                },
            },
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        init
    }
}

#[derive(Debug)]
pub enum Msg {
    SearchChanged(String),
    ToggleSource(Chip, bool),
    ToggleDefaults(bool),
    ShowConflicts,
    SaveCheatsheet,
    CopyCheatsheet,
    Reload,
}

pub struct App {
    all: Vec<Binding>,
    search: String,
    hidden: HashSet<Chip>,
    hide_defaults: bool,
    rows: FactoryVecDeque<Row>,
    // The debouncer must outlive init for watches to stay live.
    _watcher: Option<
        notify_debouncer_full::Debouncer<
            notify_debouncer_full::notify::RecommendedWatcher,
            notify_debouncer_full::RecommendedCache,
        >,
    >,
}

impl App {
    /// Rebuild `rows` from `all` through the active filters, grouped by context.
    fn repopulate(&mut self) {
        let needle = self.search.to_lowercase();
        let mut filtered: Vec<&Binding> = self
            .all
            .iter()
            .filter(|b| !self.hidden.contains(&chip_of(&b.source)))
            .filter(|b| !(self.hide_defaults && b.is_default))
            .filter(|b| {
                needle.is_empty()
                    || b.combo.to_string().to_lowercase().contains(&needle)
                    || b.action.to_lowercase().contains(&needle)
                    || b.description
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&needle)
            })
            .collect();
        filtered.sort_by(|a, b| {
            (a.context.rank(), a.source.rank(), a.sort_key()).cmp(&(
                b.context.rank(),
                b.source.rank(),
                b.sort_key(),
            ))
        });

        let mut guard = self.rows.guard();
        guard.clear();
        let mut last_ctx: Option<Context> = None;
        for b in filtered {
            if last_ctx.as_ref() != Some(&b.context) {
                last_ctx = Some(b.context.clone());
                guard.push_back(Row::Header(b.context.label()));
            }
            guard.push_back(Row::Entry {
                combo: b.combo.to_string(),
                action: b.action.clone(),
                desc: b.description.clone().unwrap_or_default(),
                badge: b.source.label().to_string(),
                dim: b.is_default,
            });
        }
    }

    /// Deterministic conflict listing for the dialog body.
    fn conflicts_text(&self) -> String {
        let mut conflicts = find_conflicts(&self.all);
        conflicts.sort_by(|a, b| {
            let kind_rank = |k: &Kind| match k {
                Kind::Exact => 0,
                Kind::FishPrefix => 1,
            };
            (a.combo.to_string(), kind_rank(&a.kind))
                .cmp(&(b.combo.to_string(), kind_rank(&b.kind)))
        });
        if conflicts.is_empty() {
            return "No conflicting bindings.".to_string();
        }
        let mut out = String::new();
        for c in &conflicts {
            let kind = match c.kind {
                Kind::Exact => "exact",
                Kind::FishPrefix => "fish prefix shadows",
            };
            let claimants: Vec<String> = c
                .bindings
                .iter()
                .map(|b| format!("{} — {}", b.source.label(), b.action))
                .collect();
            out.push_str(&format!(
                "• {}  ({kind})\n    {}\n",
                c.combo,
                claimants.join("\n    ")
            ));
        }
        out
    }

    fn window(&self) -> Option<gtk::Window> {
        self.rows
            .widget()
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok())
    }
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = ();
    type Input = Msg;
    type Output = ();

    view! {
        adw::Window {
            set_title: Some("KeyMap"),
            set_default_size: (900, 640),

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                adw::HeaderBar {},

                gtk::SearchEntry {
                    set_margin_all: 8,
                    set_placeholder_text: Some("Search bindings…"),
                    connect_search_changed[sender] => move |entry| {
                        sender.input(Msg::SearchChanged(entry.text().to_string()));
                    },
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,
                    set_margin_start: 8,
                    set_margin_end: 8,
                    set_margin_bottom: 4,

                    gtk::ToggleButton {
                        set_label: "Niri",
                        set_active: true,
                        connect_toggled[sender] => move |btn| {
                            sender.input(Msg::ToggleSource(Chip::Niri, btn.is_active()));
                        },
                    },
                    gtk::ToggleButton {
                        set_label: "Dms",
                        set_active: true,
                        connect_toggled[sender] => move |btn| {
                            sender.input(Msg::ToggleSource(Chip::Dms, btn.is_active()));
                        },
                    },
                    gtk::ToggleButton {
                        set_label: "Ghostty",
                        set_active: true,
                        connect_toggled[sender] => move |btn| {
                            sender.input(Msg::ToggleSource(Chip::Ghostty, btn.is_active()));
                        },
                    },
                    gtk::ToggleButton {
                        set_label: "Fish",
                        set_active: true,
                        connect_toggled[sender] => move |btn| {
                            sender.input(Msg::ToggleSource(Chip::Fish, btn.is_active()));
                        },
                    },
                    gtk::ToggleButton {
                        set_label: "Custom",
                        set_active: true,
                        connect_toggled[sender] => move |btn| {
                            sender.input(Msg::ToggleSource(Chip::Custom, btn.is_active()));
                        },
                    },
                    gtk::ToggleButton {
                        set_label: "Hide defaults",
                        set_active: false,
                        connect_toggled[sender] => move |btn| {
                            sender.input(Msg::ToggleDefaults(btn.is_active()));
                        },
                    },
                },

                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hexpand: true,

                    #[local_ref]
                    rows_box -> gtk::ListBox {
                        set_selection_mode: gtk::SelectionMode::Single,
                    }
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_margin_all: 8,

                    gtk::Button {
                        set_label: "Conflicts",
                        connect_clicked => Msg::ShowConflicts,
                    },
                    gtk::Button {
                        set_label: "Generate Cheat Sheet",
                        connect_clicked => Msg::SaveCheatsheet,
                    },
                    gtk::Button {
                        set_label: "Copy",
                        connect_clicked => Msg::CopyCheatsheet,
                    },
                },
            },
        }
    }

    fn init(
        _: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let rows = FactoryVecDeque::builder().launch_default().detach();

        let mut model = App {
            all: load_all(),
            search: String::new(),
            hidden: HashSet::new(),
            hide_defaults: false,
            rows,
            _watcher: None,
        };
        model.repopulate();

        let rows_box = model.rows.widget();
        let widgets = view_output!();

        // Watch config dirs; each debounced event becomes Msg::Reload.
        let (watch_tx, watch_rx) = relm4::channel();
        model._watcher = spawn_watcher(watch_tx).ok();
        let input = sender.input_sender().clone();
        relm4::spawn_local(async move {
            watch_rx.forward(input, |_| Msg::Reload).await;
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Msg::SearchChanged(text) => {
                self.search = text;
                self.repopulate();
            }
            Msg::ToggleSource(chip, shown) => {
                if shown {
                    self.hidden.remove(&chip);
                } else {
                    self.hidden.insert(chip);
                }
                self.repopulate();
            }
            Msg::ToggleDefaults(hide) => {
                self.hide_defaults = hide;
                self.repopulate();
            }
            Msg::Reload => {
                self.all = load_all();
                self.repopulate();
            }
            Msg::ShowConflicts => {
                let dialog = adw::AlertDialog::builder()
                    .heading("Conflicts")
                    .body(self.conflicts_text())
                    .build();
                dialog.add_response("close", "Close");
                dialog.set_default_response(Some("close"));
                dialog.set_close_response("close");
                dialog.present(self.window().map(|w| w.upcast::<gtk::Widget>()).as_ref());
            }
            Msg::SaveCheatsheet => {
                let markdown = render_markdown(&self.all);
                let dialog = gtk::FileDialog::builder()
                    .title("Save Cheat Sheet")
                    .initial_name("keymap-cheatsheet.md")
                    .build();
                let window = self.window();
                relm4::spawn_local(async move {
                    if let Ok(file) = dialog.save_future(window.as_ref()).await {
                        if let Some(path) = file.path() {
                            let _ = std::fs::write(path, markdown);
                        }
                    }
                });
            }
            Msg::CopyCheatsheet => {
                self.rows
                    .widget()
                    .clipboard()
                    .set_text(&render_markdown(&self.all));
            }
        }
    }
}
