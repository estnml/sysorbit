mod host;
mod i18n;
mod model;
mod ssh;
mod systemd;
mod timeline;
mod ui;

use std::sync::Arc;

use host::{Host, LocalHost};
use ssh::{SshHost, Target};

/// `sysorbit` shows this machine's timers;
/// `sysorbit [ssh://][user@]host[:port] [-o option]...` those of a Linux
/// host reached over SSH, passing the `-o` options on to `ssh`.
fn main() -> iced::Result {
    i18n::init();
    let mut args = std::env::args().skip(1);
    let host: Arc<dyn Host> = match args.next() {
        Some(destination) => {
            let mut target = Target::parse(&destination);
            while let Some(arg) = args.next() {
                match (arg.as_str(), args.next()) {
                    ("-o", Some(option)) => target.options.push(option),
                    _ => eprintln!("sysorbit: ignoring argument {arg}"),
                }
            }
            Arc::new(SshHost::new(target))
        }
        None => Arc::new(LocalHost::new()),
    };
    ui::run(host)
}
