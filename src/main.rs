mod conflict;
mod load;
mod model;
mod normalize;
mod parsers;
mod report;
mod ui;
mod watch;

use relm4::RelmApp;

fn main() {
    let app = RelmApp::new("com.github.keymap");
    app.run::<ui::App>(());
}
