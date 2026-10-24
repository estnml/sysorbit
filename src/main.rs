mod diagnostics;
mod host;
mod hosts;
mod i18n;
mod model;
mod ssh;
mod systemd;
mod timeline;
mod ui;

use hosts::HostEntry;
use ssh::Target;

/// `sysorbit` shows the host shown last time, else this machine's timers;
/// `sysorbit [ssh://][user@]host[:port] [-o option]...` those of a Linux
/// host reached over SSH, passing the `-o` options on to `ssh`. Either way
/// the host menu offers every known host.
fn main() -> iced::Result {
    i18n::init();
    let _logging = diagnostics::init();
    let (mut hosts, problems) = hosts::load();

    let mut args = std::env::args().skip(1);
    let selected = match args.next() {
        Some(destination) => {
            let mut target = Target::parse(&destination);
            while let Some(arg) = args.next() {
                match (arg.as_str(), args.next()) {
                    ("-o", Some(option)) => target.options.push(option),
                    _ => eprintln!("sysorbit: ignoring argument {arg}"),
                }
            }
            hosts.push(HostEntry::from_command_line(target));
            Some(hosts.len() - 1)
        }
        None => hosts::last_host()
            .and_then(|id| hosts.iter().position(|host| host.id == id))
            .or_else(|| (!hosts.is_empty()).then_some(0)),
    };

    let result = ui::run(hosts, selected, problems);
    diagnostics::event(diagnostics::Event::Exit);
    result
}
