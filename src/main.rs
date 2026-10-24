mod host;
mod i18n;
mod model;
mod systemd;
mod timeline;
mod ui;

use std::sync::Arc;

use host::LocalHost;

fn main() -> iced::Result {
    i18n::init();
    ui::run(Arc::new(LocalHost::new()))
}
