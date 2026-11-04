#!/usr/bin/env python3
"""Packages the browser build for players, as Eaglercraft ships: a web folder
to put on any static host.

  web/out/horizon-web/   the web folder: upload it as is (GitHub Pages,
                         Netlify, any server). Its service worker gives the
                         page the headers the game's threads need.

Usage:
  web/package.py [--no-build]

It builds first (web/build.sh into web/dist); PROFILE=fast and the other
build.sh variables pass through. The folder always has every resource: its
files are fetched when the game first reads them.
"""

import argparse
import hashlib
import os
import shutil
import subprocess
from pathlib import Path

WEB = Path(__file__).resolve().parent
DIST = WEB / "dist"
PAGE_FILES = ("index.html", "main.js", "audio.js", "worker.js", "sw.js")


def build() -> None:
    subprocess.run([str(WEB / "build.sh")], env=dict(os.environ), check=True)


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()[:16]


def folder(out: Path) -> None:
    site = out / "horizon-web"
    if site.exists():
        shutil.rmtree(site)
    site.mkdir(parents=True)
    for name in PAGE_FILES:
        shutil.copy2(WEB / name, site / name)
    shutil.copytree(DIST / "pkg", site / "pkg")
    fs = site / "fs"
    fs.mkdir()
    for path in (DIST / "fs").iterdir():
        if path.is_file():
            shutil.copy2(path, fs / path.name)
    # The files fetched one by one: a packed folder's come with its pack.
    res, count = DIST / "fs" / "res", 0
    for line in (fs / "manifest.txt").read_text().splitlines():
        rel, *rest = line.split("\t")
        if len(rest) > 1:
            continue
        (fs / "res" / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(res / rel, fs / "res" / rel)
        count += 1
    files = ("pkg/horizon-client_bg.wasm", "fs/boot.tar.gz")
    (site / "build.json").write_text("{" + ", ".join(f'"{f}": "{digest(site / f)}"' for f in files) + "}\n")
    size = sum(p.stat().st_size for p in site.rglob("*") if p.is_file())
    print(f"{site}: {size / 1e6:.0f} MB ({count} files fetched when read)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--no-build", action="store_true", help="package what web/dist holds")
    parser.add_argument("--out", type=Path, default=WEB / "out")
    args = parser.parse_args()
    if not args.no_build:
        build()
    args.out.mkdir(parents=True, exist_ok=True)
    folder(args.out)


if __name__ == "__main__":
    main()
