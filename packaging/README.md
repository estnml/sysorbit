# Linux packages

The initial packages target x86_64 Linux: DEB, RPM and an AUR `sysorbit-bin`
recipe. They install a compiled executable, icons and a desktop menu entry.
Rust is not needed on the user's computer.

The shared executable is built on Debian 12 (glibc 2.36). Debian 12+, Ubuntu
24.04+, recent Fedora and Arch Linux meet this baseline. This does not imply
compatibility with every RPM-based distribution. ARM packages and macOS/Windows
installers are outside this initial packaging flow.

## Build

From the repository root, with Docker installed:

```sh
docker build -t sysorbit-packaging:local -f packaging/Dockerfile packaging
mkdir -p target/linux-packaging
docker run --rm --user "$(id -u):$(id -g)" \
  -e CARGO_HOME=/tmp/cargo \
  -v "$PWD:/work:ro" \
  -v "$PWD/target/linux-packaging:/work/target" \
  sysorbit-packaging:local
```

Outputs are in `target/linux-packaging/packages/`. The pinned build tools are
cargo-packager (DEB) and cargo-generate-rpm (RPM); cargo-packager does not support
RPM. `prepare-aur.py` creates a binary archive and inserts its actual SHA-256
into the AUR recipe. Build from a clean release checkout when preparing a release.
RPM dependencies are declared explicitly in `Cargo.toml`: the pinned RPM tool's
automatic detector misreads optional glibc symbols as required capabilities.

On Arch, complete and test the AUR recipe without publishing it:

```sh
cd target/linux-packaging/packages/aur
cp ../sysorbit-*-linux-x86_64.tar.gz .
makepkg --printsrcinfo > .SRCINFO
makepkg
```

`makepkg` requires the `base-devel` package group and the recipe's runtime
dependencies. The local archive lets this test run before a GitHub Release exists.

The Linux packages workflow performs the same build and prepares `.SRCINFO` and
an Arch package. Run by hand, it only uploads Actions artifacts. For a `v*` tag
that passes every installation test, it also creates a draft GitHub Release
with the packages; it never publishes the release or submits to AUR.
Installation tests launch the desktop entry in an isolated Xvfb session on
Debian 12, Fedora 43 and Arch. They check window creation with software rendering;
they do not exercise a real Wayland session or GPU driver.

## Install a downloaded package

Debian/Ubuntu:

```sh
sudo apt install ./sysorbit_0.1.0_amd64.deb
```

Fedora:

```sh
sudo dnf install ./sysorbit-0.1.0-1.x86_64.rpm
```

Arch, using a locally built package:

```sh
sudo pacman -U ./sysorbit-bin-0.1.0-1-x86_64.pkg.tar.zst
```

Use the filename of the version you downloaded. Open **Sysorbit** from the
desktop application menu after installation. The launcher uses the same local
configuration and SSH agent as the desktop session.

## Publication

Set the version in `Cargo.toml`, commit, then tag and push that commit:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The workflow checks that the tag matches `Cargo.toml`, builds and tests the
packages, and creates a draft release `v0.1.0` with the DEB, RPM, Arch package,
`sysorbit-VERSION-linux-x86_64.tar.gz`, `PKGBUILD` and `.SRCINFO`. Review the
notes on GitHub and publish it. The archive in the release is the one the AUR
checksum was made from; do not replace it.

Submit the release's `PKGBUILD`, `.SRCINFO` and the repository's `LICENSE` to the
`sysorbit-bin` AUR repository separately. Only then will `yay -S sysorbit-bin`
work.

AUR updates require a new recipe and checksum for each release. AUR availability
does not put a package in Arch's official repositories, so `pacman -S sysorbit`
would require separate repository inclusion. Downloaded DEB/RPM packages also
require manual updates until an APT/DNF repository is provided.
