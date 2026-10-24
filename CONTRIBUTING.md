# Contributing to sysorbit

Thank you for helping. Bug reports, ideas, translations and code are all
welcome.

## Reporting bugs and ideas

Open an [issue](https://github.com/estnml/sysorbit/issues/new/choose) and
pick a template. For a bug, the sysorbit version, your operating system,
whether the host is this machine or reached over SSH, and its systemd
version help most. The app's log files are safe to attach: they hold
codes, counts and pseudonyms, never host or unit names. See
[LOGGING.md](LOGGING.md) for where they are.

Report security problems privately instead; see [SECURITY.md](SECURITY.md).

## Translating

Translations need no Rust. [TRANSLATING.md](TRANSLATING.md) explains how
to add or improve a language.

## Changing the code

With a stable Rust toolchain:

```sh
cargo run                 # this machine's timers, on Linux
cargo run -- user@host    # a Linux host over SSH
```

Before opening a pull request, run what CI runs on Linux, macOS and
Windows:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The tests include the interface: `iced_test` drives it headlessly, the
way iced's runtime does, so a test can click, type and check focus or
layout without a display. Tests that need a real systemd, a container or
the demo fleet are marked `#[ignore]` and say how to run them.

To try changes against many hosts, `tests/fleet/run.sh` starts a demo
fleet of Linux distributions with sshd and a varied set of timers, listed
in the app's host menu. `tests/fleet/run.sh stop` removes it.

After changing the interface, retake the README's screenshots with the
fleet running:

```sh
SYSORBIT_SCREENSHOTS=assets/screenshots cargo test readme_screenshots -- --ignored
```

### Style

- Match the code around you: its naming, comment density and idioms.
  Comments say why, not what.
- Add or update tests with behaviour changes.
- Interface text goes into `i18n/en/sysorbit.ftl`. Add the Turkish text to
  `i18n/tr/sysorbit.ftl` if you can; otherwise leave it out and English is
  shown.
- Commit messages have a short imperative subject ("Fix the Windows build
  of the SSH bridge socket") and a body in plain sentences explaining what
  changed and why.

## Packaging

[packaging/README.md](packaging/README.md) describes how the Linux packages
and the macOS disk image are built, tested and released.

## License

By contributing, you agree that your contributions are licensed under the
project's [MIT license](LICENSE).
