# Security policy

## Supported versions

Security fixes go into the latest release. Please check that a problem
still exists there, or on `main`, before reporting it.

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's
[private vulnerability reporting](https://github.com/estnml/sysorbit/security/advisories/new),
not in a public issue. Include what an attacker could do, the steps to
reproduce it, and the sysorbit version and operating system you used.

sysorbit is maintained by one person. You can expect an acknowledgement
within a week and a fix as soon as one is ready; you will be credited in
the advisory unless you prefer otherwise.

## What is in scope

sysorbit acts with the privileges of the person using it, and sometimes
more, so these areas matter most:

- **Creating timers.** Unit files are written by a shell script that runs
  as root through `pkexec` on this machine and `sudo -n` over SSH. Anything
  that lets form input escape into the script, the unit files or the
  systemd settings they contain is a vulnerability.
- **Remote hosts.** Commands run over the system's OpenSSH client, with
  every argument quoted for the remote shell. Injection through host
  entries, `hosts.toml`, `~/.ssh/config` aliases or command-line arguments
  is in scope.
- **Local logs.** [LOGGING.md](LOGGING.md) promises that host names, unit
  names, command output and other private data never reach the log files.
  A record that leaks them is a vulnerability.
- **Release artifacts.** The Linux packages and the macOS disk image built
  by the packages workflow, and the AUR recipe.

Bugs in systemd, OpenSSH, polkit or sudo themselves belong to those
projects.
