# sysorbit

A cross-platform desktop application for Linux, macOS, and Windows to visualize
systemd timers (239+) on local and remote Linux hosts. Its music-box timeline
brings schedules, past runs and upcoming triggers into one view.

Built with Rust and [iced](https://iced.rs).

## Features

- Browse system and user timers with live updates.
- Explore schedules, run history and journal logs.
- Connect to Linux hosts over SSH and switch between saved hosts.
- View the interface in English or Turkish.
- Keep privacy-preserving local logs for bug reports.

Linux supports both local and remote hosts. macOS and Windows connect to Linux
hosts over SSH. The application supports systemd 239 and newer, with reduced
schedule detail on older versions.

## Installation

Linux packaging is available for DEB, RPM and Arch/AUR on x86_64, including an
application menu entry and icon. See [Linux packaging](packaging/README.md) for
build and installation instructions. These packages have not been published to
GitHub Releases or AUR yet. macOS and Windows currently require a source build
or a CI binary.

## Build and run

With a stable Rust toolchain installed:

```sh
git clone https://github.com/estnml/sysorbit.git
cd sysorbit
cargo run --release
```

To open a remote host:

```sh
cargo run --release -- user@server.example.com
```

Remote connections use the local OpenSSH client and its configuration. Set up
key-based authentication or an SSH agent first; password prompts are not
supported. The remote host needs systemd, `journalctl`, `systemd-analyze` and
`systemd-stdio-bridge`. Journal access follows the connecting user's permissions.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI builds and tests all three platforms. Development binaries are attached as
artifacts to successful [GitHub Actions runs](https://github.com/estnml/sysorbit/actions).

See [LOGGING.md](LOGGING.md) for local diagnostics and privacy details, and
[TRANSLATING.md](TRANSLATING.md) to contribute a translation.

Created by [Esat Mustafa Namlı (@estnml)](https://github.com/estnml).

Licensed under the [MIT license](LICENSE).
