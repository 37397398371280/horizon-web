#!/usr/bin/env bash
# Builds Horizon for the browser into web/dist: the game as WebAssembly, the
# page, and the prepared resources packed for download.
#
#   web/build.sh            the release build: LTO, then wasm-opt (binaryen) when installed
#   PROFILE=fast web/build.sh  optimized, no LTO or wasm-opt: quick to rebuild while working
#   WASM_OPT=0 web/build.sh skip wasm-opt
#   DIST=<folder> web/build.sh  build into another folder
#   BUNDLE=0 web/build.sh   only the game and the page, not the resources
#
# Threads are Web Workers on shared memory, which std only supports when it is
# rebuilt with atomics: that build needs the pinned nightly below with its
# rust-src (rustup component add rust-src --toolchain <it>).
# Needs: rustup target add wasm32-unknown-unknown, and the wasm-bindgen CLI of
# the version in Cargo.lock (cargo install wasm-bindgen-cli --version <it>); for
# the release build, wasm-opt (binaryen's package, e.g. pacman -S binaryen).
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${PROFILE:-release}"
resources="${HORIZON_RESOURCES:-.local/horizon-client/26.3}"
dist="${DIST:-web/dist}"

nightly=nightly-2026-10-14
# Replaces .cargo/config.toml's flags for the target, so it repeats them.
# The linker flags give the module a shared memory the page hands each
# worker, and the thread-local storage symbols wasm-bindgen sets up per thread.
flags=(
    '--cfg getrandom_backend="wasm_js"'
    -C target-feature=+atomics,+bulk-memory,+mutable-globals
    -C link-arg=--shared-memory -C link-arg=--import-memory
    -C link-arg=--max-memory=4294967296
    # The page's own stack (workers get theirs from horizon_os): rustc's 1 MiB
    # overflowed into the heap when the loading screen's future grew, and the
    # page died with "memory access out of bounds" before the title screen.
    -C link-arg=-zstack-size=8388608
    -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size
    -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base
)
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="${flags[*]}"
cargo=(cargo "+$nightly" -Z build-std=std,panic_abort)
"${cargo[@]}" build --target wasm32-unknown-unknown --profile "$profile" \
    -p horizon-client --bin horizon-client --no-default-features --features audio,controllers

target_dir="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
mkdir -p "$dist/pkg"
bindgen=(wasm-bindgen --target web --no-typescript --out-dir "$dist/pkg")
# Function names (5 MB) only make stack traces readable: the release build drops them.
[ "$profile" = release ] && bindgen+=(--remove-name-section)
"${bindgen[@]}" "$target_dir/wasm32-unknown-unknown/$profile/horizon-client.wasm"
wasm="$dist/pkg/horizon-client_bg.wasm"
if [ "$profile" = release ] && [ "${WASM_OPT:-1}" != 0 ]; then
    if command -v wasm-opt > /dev/null; then
        # The features rustc's wasm32 target turns on, and the threaded build's atomics.
        wasm-opt -O2 --enable-threads --enable-bulk-memory --enable-mutable-globals \
            --enable-sign-ext --enable-nontrapping-float-to-int --enable-reference-types \
            --enable-multivalue "$wasm" -o "$wasm.opt"
        mv "$wasm.opt" "$wasm"
    else
        echo "wasm-opt not found: the module is left as rustc made it (install binaryen)" >&2
    fi
fi
cp web/index.html web/main.js web/audio.js web/worker.js web/sw.js "$dist/"

[ "${BUNDLE:-1}" = 0 ] && { ls -l "$dist/pkg"/*.wasm; exit 0; }
if [ ! -f "$dist/fs/boot.tar.gz" ] || [ "$resources/horizon-resources.json" -nt "$dist/fs/boot.tar.gz" ] \
    || [ web/bundle.py -nt "$dist/fs/boot.tar.gz" ]; then
    python3 web/bundle.py "$resources" "$dist/fs"
fi
# The large files' content hashes: the page asks for them with these, so the
# service worker (sw.js) can keep them until they change.
python3 - "$dist" <<'PY'
import hashlib, json, sys
from pathlib import Path
dist = Path(sys.argv[1])
def digest(rel):
    h = hashlib.sha256()
    with open(dist / rel, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()[:16]
files = {rel: digest(rel) for rel in ("pkg/horizon-client_bg.wasm", "fs/boot.tar.gz")}
(dist / "build.json").write_text(json.dumps(files) + "\n")
PY
ls -l "$dist/pkg"/*.wasm
echo "Serve it with: python3 web/serve.py, then open http://localhost:8080"
