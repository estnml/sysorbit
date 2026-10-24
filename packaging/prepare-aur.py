#!/usr/bin/env python3
"""Prepare a versioned binary archive and an AUR recipe with its real checksum."""

import gzip
import hashlib
import os
import re
import shutil
import tarfile
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parent.parent
manifest = tomllib.loads((root / "Cargo.toml").read_text())
version = manifest["package"]["version"]
if not re.fullmatch(r"\d+\.\d+\.\d+", version):
    raise SystemExit("The initial packaging flow requires a stable x.y.z version.")
output = root / "target/packages"
output.mkdir(parents=True, exist_ok=True)
archive = output / f"sysorbit-{version}-linux-x86_64.tar.gz"
files = {
    "sysorbit": root / "target/release/sysorbit",
    "io.github.estnml.sysorbit.desktop": root / "packaging/io.github.estnml.sysorbit.desktop",
    "sysorbit.svg": root / "assets/sysorbit.svg",
    "sysorbit.png": root / "assets/sysorbit.png",
    "LICENSE": root / "LICENSE",
    "README.md": root / "README.md",
    "LOGGING.md": root / "LOGGING.md",
}
# Fixed timestamps make the archive, and so the AUR checksum, depend only on
# its contents. SOURCE_DATE_EPOCH follows the reproducible-builds convention.
epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
with archive.open("wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", mtime=epoch) as compressed:
    with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as bundle:
        for name, source in files.items():
            entry = bundle.gettarinfo(str(source), arcname=f"sysorbit-{version}/{name}")
            entry.uid = entry.gid = 0
            entry.uname = entry.gname = "root"
            entry.mode = 0o755 if name == "sysorbit" else 0o644
            entry.mtime = epoch
            with source.open("rb") as content:
                bundle.addfile(entry, content)
checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
aur = output / "aur"
aur.mkdir(exist_ok=True)
template = (root / "packaging/aur/PKGBUILD.in").read_text()
(aur / "PKGBUILD").write_text(template.replace("@VERSION@", version).replace("@SHA256@", checksum))
shutil.copyfile(root / "LICENSE", aur / "LICENSE")
print(f"Created {archive.name} and aur/PKGBUILD with SHA-256 {checksum}")
print("Generate .SRCINFO with makepkg --printsrcinfo before submitting to AUR.")
