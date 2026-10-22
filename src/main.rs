mod host;
mod model;
mod systemd;
mod timeline;
mod ui;

use std::sync::Arc;

use host::LocalHost;

fn main() -> iced::Result {
    ui::run(Arc::new(LocalHost::new()))
}
