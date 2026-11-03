# Horizon in the browser

An experimental build of the Horizon client for the web: the same game code
compiled to WebAssembly, drawing through the browser's WebGPU. It creates and
opens single-player worlds, joins offline-mode Horizon and Eaglercraft
servers, shares worlds with other browsers through Eaglercraft's relays,
plays sound, reads controllers, and keeps the worlds, the player's profile
and the options in the browser's storage (see [What's missing](#whats-missing)).

As in Eaglercraft, the first screen is **Edit Profile**: the player's name,
a skin (one of the game's nine default skins with classic or slim arms, or a
64×64 picture imported from a file) and an imported cape, with the player
drawn in them. The title screen's Edit Profile button (where Realms is on
native) opens it again. **Open to LAN** shares the world: the chat gives a
join code, which other players type in **Join Shared World** (the title
screen's Friends button).

## Build and run

You need the prepared resources in `.local/horizon-client/26.3` (see the main
README's *Build from source*), and once:

```sh
rustup target add wasm32-unknown-unknown
rustup toolchain install nightly-2026-10-14 --component rust-src   # threads: std rebuilt with atomics
cargo install wasm-bindgen-cli --version 0.2.129   # the version in Cargo.lock
# and binaryen (wasm-opt, for the release build) from your package manager: pacman -S binaryen, apt install binaryen, brew install binaryen
```

Sounds come with the prepared resources when your Minecraft install has them
(`horizon-prepare sounds` adds them to resources prepared without).

Then:

```sh
web/build.sh        # the game to web/dist/pkg, the resources to web/dist/fs
python3 web/serve.py
```

and open <http://localhost:8080> in a browser with WebGPU. Chrome and Edge have it
on Windows, macOS and ChromeOS. On Linux, Chromium-based browsers need it switched
on: `brave --enable-unsafe-webgpu --enable-features=Vulkan http://localhost:8080`
(or the same flags for `chromium` / `google-chrome`).

`web/build.sh` makes the release build: LTO, then `wasm-opt` when it is installed
(slow to build; what players should get). `PROFILE=fast web/build.sh` skips both and
rebuilds in about a minute while you work. `THREADS=0 web/build.sh` builds with
stable Rust and no threads.

### Packages for players: web folder and offline file

Like Eaglercraft, the game ships two ways:

```sh
web/package.py              # both, into web/out (builds threaded and single-threaded first)
web/package.py folder       # web/out/horizon-web/: upload to any static host
web/package.py offline      # web/out/horizon-offline.html: one file, opened from the disk
web/package.py offline --sounds all --languages   # with the music and every language
```

- **`horizon-web/`** works on any static host (GitHub Pages, Netlify, a plain
  web server). Hosts that can't send the isolation headers get them from the
  service worker, which reloads the page once on the first visit; where even
  that fails (no service worker), the page runs the single-threaded build in
  `pkg-st/`. Files the game reads later are fetched from `fs/res/` when read.
- **`horizon-offline.html`** holds the single-threaded build and the resources
  as base64 (a `file://` page can't fetch or be cross-origin isolated). Single
  player runs there on green threads (below), which need WebAssembly stack
  switching (JSPI: Chrome and Edge 137+). By default
  it holds every sound but the music and music discs, and English only:
  `--sounds none|effects|all` and `--languages` change that. The files it holds
  for later are handed to the game's filesystem when read
  (`globalThis.horizonHeldFile`, asked by `horizon_fetch_sync`). Saves go to the
  browser's storage for `file://` pages, shared by every offline file.

`--no-build` packages what `web/dist` and `web/dist-st` hold; `PROFILE=fast`
passes through to the builds.

### Without shared memory: green threads

The single-threaded build can't start Web Workers on the game's memory. Single
player still needs its server thread, which talks to the window through a
connection in memory with a thread at each end. There those threads are green
threads on the page's own (`horizon-os`'s `fibers.rs`). Each runs on a stack of
its own, entered through `WebAssembly.promising`. When it waits in
`horizon_os::thread::sleep` or a `horizon_os::sync` helper, it suspends through
a `WebAssembly.Suspending` import until a timer resumes it, and the page draws
meanwhile. Rust keeps part of each stack in linear memory: the build exports
`__stack_pointer`, and the scheduler switches it with the green thread.

std's own waits (`recv()`, `Condvar::wait`, `park`) return at once without
threads, so a loop on them would hold the page forever. Only the threads listed
in `fibers.rs`, whose waits all go through `horizon_os`, start as green threads;
the others fail as before and the game does their work on the page.
`globalThis.horizonFibers` in the console shows each green thread and what it
waits for. The window can't wait for its server there (`Integrated::call` looks once):
World Options asks with `Integrated::ask` and opens when the answer comes
(`poll_world_options`); work it only hands over (applying options) runs as is.
Open to LAN says "Unable to host local game": sharing a world needs the
threaded build's sockets.

### Browsers

The game needs WebGPU, and the threaded build a cross-origin isolated page
(`web/serve.py` sends the headers; any host must send
`Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp`, or serve the packaged web folder,
whose service worker adds them):

A browser without WebGPU, or with no GPU it can use, gets a message saying so
instead of the game.

The address passes the game its command-line arguments:
<http://localhost:8080/?args=--world%20Test%20--creative> opens (or creates) the
world `Test` in creative mode at start.

## How it fits together

- **`horizon-os`** is what the game sees of the operating system: `fs`, `time`,
  `thread`, `sync`, `env` and `process`. On native they are std's own; in the
  browser, `fs` is a virtual filesystem in memory, `time` is the browser's
  clock, threads are Web Workers, and scoped threads run one after another. The game's crates call `horizon_os::fs`
  where they called `std::fs`, and `path.fs_exists()` (`PathExt`) where they
  called `path.exists()`.
- **The resources** go to the page as one archive: `web/bundle.py` packs
  everything the game reads to reach the title screen and open a world into
  `boot.tar.gz` (about 15 MB). The rest (other languages, the world generator's
  bundle) is listed in `manifest.txt` and fetched the first time it is read.
  The world generator's folders are read together, some 500 files when a
  world starts, so `bundle.py` also packs each whole (`worldgen.tar.gz`, 4 MB
  for 34 MB of files): the manifest names the pack for each of its files, and
  the first read fetches and unpacks all of them in one request (each file
  alone if that fails). With 40 ms of latency a new world's player spawned
  12 s after the title's first frame with a request per file, 4 s with the pack.
- **Startup** (`web/main.js`): check for WebGPU, start the service worker, load
  the WebAssembly, download and install the archive, load the player's saved
  files, then `horizon_start()`. The resources load on a worker, as on native,
  while the first renderer opens asynchronously (`Loading::resumed`, because
  the page can't wait for the GPU) and the loading screen draws: it shows
  0.4 s after start instead of after the 1.5 s load.
- **Frame pacing**: the page never polls winit's loop (`ControlFlow::Poll`
  turned it some 100,000 times a second between animation frames, a whole
  core): after each frame it asks for the next and waits for it
  (`web_wait`). The frame limiter can't wait on the page either, so instead
  of `limit_display_fps` the redraw skips the animation frames that come
  before the limit's slot (`FramerateLimiter::frame_due`). In a world at
  60 fps the page's thread is busy about 60–100 ms a second.
- **Threads** (`horizon-os/src/workers.rs`, `web/worker.js`): the module is
  built with atomics on a shared memory (std rebuilt on nightly, the linker's
  `--shared-memory`). Each spawned thread is a module worker that instantiates
  the same module on that memory and runs the closure it was handed. Workers
  are always started by the page, since a worker's own JS is busy running its
  thread. The page's thread never waits: browsers trap a wait there, so
  `sleep` returns at once and `join` spins. std's `recv_timeout` and
  `wait_timeout_while` read a clock std doesn't have here: `horizon_os::sync`
  has versions that use the browser's.
- **Locks the page shares with workers**: std's `Mutex` waits once it has spun
  briefly, which traps on the page (`Atomics.wait cannot be called in this
  context`; winit swallows it and the frame loop stops). Those locks go through
  `horizon_os::sync::lock` and `write`, which spin on the page instead: the
  chunk queue, the virtual filesystem, and the single-player connection's
  in-memory pipe and writer (the page's packets and the network thread's
  keep-alives).
- **Single player** runs as on native: the server thread, its local connection
  and the client's network thread are Web Workers. The window's calls into its
  server (`Integrated::call`) spin on the page for the reply
  (`horizon_os::sync::recv_reply`).
- **Saving**: what the game writes outside `/tmp` and `/.local` (options,
  `saves/`, `servers.dat`) is also kept in the origin's private file system
  (OPFS), in a `horizon` folder laid out as in the virtual filesystem. The
  page loads that folder before the game starts; the virtual filesystem notes
  each path the game changes, and every 2 s (and when the tab is hidden) the
  page takes those paths with their state now (`horizon_take_changes`) and
  writes them with OPFS's async API, which the page's thread may use. Each
  file is swapped in whole, but a batch isn't: a tab closed mid-batch can keep
  some files of a save and not others. What the game hasn't saved yet (between
  autosaves) is lost with the tab, as with a crash on native.
- **Pointer lock**: browsers lock the pointer only during a click or a key
  press, and winit's request throws through the frame when refused. The game
  asks through `horizon_os::web::pointer_lock`, and the canvas takes the lock
  on the player's next click. Escape while the pointer is locked only releases
  the lock and never reaches the page, so the game takes a lost lock during
  play as that Escape and pauses (`horizon_os::web::pointer_lock_lost`).
- **Sound**: cpal's Web Audio backend (`audio.rs` mixes as on native). Its
  `AudioContext` starts suspended until the player clicks or presses a key;
  `web/audio.js` wraps the class before the game's glue loads (the glue keeps
  the one it finds) and resumes every context on the first click or key.
  Sound files are in `manifest.txt`, fetched when a sound first plays, by the
  decoder and music workers. The mixer and decoded-sound locks are shared
  between the page (cpal's callback) and those workers, so they go through
  `horizon_os::sync::lock`.
- **Controllers**: gilrs reads the browser's Gamepad API. The DualSense's own
  output (rumble, adaptive triggers, lights) needs HID, which pages don't
  have: `horizon-dualsense` builds without it, as on iOS.
- **Caching** (`web/sw.js`): `build.sh` writes `build.json` with the content
  hashes of the WebAssembly and `boot.tar.gz`, and the page asks for them as
  `…?v=<hash>`. The service worker keeps those until a build changes them;
  everything else goes to the network first and falls back to its copy offline.
  Files fetched when read (`fs/res/`) aren't kept.
- **Multiplayer**: pages have no TCP, so the client reaches a server through a
  WebSocket to the server's own port (`horizon_os::net`): `ws://host:port/`,
  or `wss:` when the page is `https:`. The socket lives on the page, whose
  thread gets its events; the network and ping workers read it like a socket,
  waiting on a condvar, and ask the page by a message to open, flush or close
  it. All writes go into one queue the page sends from, so bytes keep their
  order whoever wrote them (the window's packets and the network thread's
  keep-alives). Horizon's servers (dedicated, and a world opened to LAN) take
  WebSockets beside Minecraft's TCP on the same port: a connection starting
  with `GET ` is upgraded (`horizon-protocol/src/websocket.rs`), then carries
  the same bytes in binary messages. Join from *Multiplayer* or with
  `?args=--server%20host:port`. Without threads (`THREADS=0`) there is no
  network worker, so no multiplayer.
- **Eaglercraft servers**: an address that is a `ws://` or `wss://` URL
  (`servers::websocket_url`) is an EaglerXServer plugin in front of a proxy,
  whose ViaVersion takes 26.3's protocol (777). The socket then goes a message
  at a time, and `horizon-protocol/src/eagler.rs` stands between it and the
  connection's byte stream. It reads the client's own handshake and login
  start, runs the plugin's login (Eagler protocol 5, which lifts the default
  1.12 cap on Minecraft protocols: client version → server version → request
  login → allow login → finish login, no skin sent), and answers with the
  login-finished packet; it drops the client's login acknowledgement, which the
  plugin sends itself. After that each game packet is one binary message
  without its length (never compressed or encrypted), and the plugin's own
  messages (`0xEE` first) are skipped. A refusal (version mismatch, denied
  login, redirect, error) becomes the login disconnect, so the client shows
  the reason. The server list's status is the plugin's `Accept: motd` text
  query, turned into a vanilla status. Checked against
  `wss://eagler.carrot-craft.org` (EaglercraftXServer 1.1.1 on Velocity):
  login, configuration, play and keep-alives.
- **Profile** (`horizon-client/src/profile.rs`, `title/profile_screen.rs`):
  the name is the offline name's line in `horizon-player.txt`, so every join
  uses it; the skin and cape are more lines there (`skin:steve`,
  `slim:true`, `cape:true`), with imported pictures in `horizon-skin.png` and
  `horizon-cape.png`. Importing goes through a file chooser
  (`horizon_os::web::pick_file`): browsers open one only during a click, so
  the button asks and the title takes the file a frame later. The skin is
  worn where the server gives none (`skins::set_own_look`, read by
  `update_remote_skins`), which is single player's offline server, shared
  worlds and offline servers. Native never sets it.
- **Eaglercraft skins**: the login sends the skin and cape as profile data
  (`skin_v2`: 2, the model, 64×64 pixels packed in 3 bytes; `cape_v1`: 2, the
  23×17 pixels of the cape that show), so other Eaglercraft players see them.
  Players the server sends no textures for are asked of the plugin
  (`0xEE`, CPacketGetOtherTexturesV5EAG); its answers (SPacketOtherTexturesV5EAG)
  give custom skins and capes, which go to the atlas's skin slots, and preset
  ids, worn as Steve or Alex (the presets' own pictures aren't here).
- **Shared worlds** (`horizon_os::net::SharedWorld`, `WebSocket::connect_shared`):
  Eaglercraft's LAN worlds. The host's page keeps a WebSocket to an
  EaglerSPRelay (the public `wss://relay.lax1dude.net/`,
  `wss://relay.shhnowisnottheti.me/` and `wss://relay.deev.is/`, or
  `sharedWorldRelays` in horizon.txt, which Join Shared World's Relay field
  sets), which gives the world a join code. A player joining sends the code
  to each relay in turn until one knows it; the relay passes one offer and
  one answer (with every ICE candidate in it) between the two pages, and a
  WebRTC data channel opens between them. The host's LAN server accepts the
  channel as it accepts a WebSocket (`lan::Listener`, `conn::serve_web`), and
  both ends carry the game's bytes in pieces of at most 16 KiB. The relay
  protocol is EaglerSPRelay's version 1; what goes over the channel is
  Horizon's own, so shared worlds are between Horizon pages, not Eaglercraft
  1.8 clients.
- **Chunk work** (`streaming.rs` on `horizon_threads::jobs`, the process's one
  job pool): the pool's workers are Web Workers, and the page never waits for
  them (a streamer leaving the pool spins until its running jobs end). Without
  threads (`THREADS=0`), the frame runs the pool's queued generation, lighting
  and meshing jobs itself (`Pool::pump`) for up to 12 ms per update.
- **Shaders**: browsers enforce WGSL rules that native wgpu doesn't
  (derivative uniformity, parentheses between `<<` and `|`); `hud.wgsl` and the
  entity shader turn the uniformity check off, which changes nothing they draw.

## What's missing

- **Other page-side locks.** Only the locks seen trapping use
  `horizon_os::sync::lock`; any other lock the page shares with a worker can
  still stop the game.
- **Screenshots and world icons**: reading a frame back needs the page to wait
  for the GPU. They report an error on the web; the world list shows no icon.
- **Moving worlds in and out**: no export or import of a world folder (a
  `.zip`) yet; a world stays in the browser that made it.
- **Online-mode servers**: Microsoft sign-in needs a backend, so the page
  joins offline-mode servers only. Java servers with neither Horizon nor the
  Eaglercraft plugin take no WebSockets: they need a proxy in front (none
  written yet).
- **Eaglercraft extras**: the server list shows no icon (the plugin's 64×64
  RGBA icon isn't read), Eagler passwords (`authEnabled`) and its cookies
  aren't offered, and the plugin's messages other than skins (voice,
  notifications, its pause menu) are dropped. Preset skins show as Steve or
  Alex, and preset capes not at all.
- **Skins in shared worlds**: a shared world's players see each other's
  default skins; only the host's own window shows the host's profile skin
  (the server sends offline profiles without textures).
- **Edit Profile's preview** is the player's front, not Eaglercraft's
  turning model. Servers whose plugin only offers Eagler 3/4, or caps
  `max_minecraft_protocol_v5` under 777, refuse the version. The native
  client can't join a `ws:` address.
- **Secure pages and plain servers**: an `https:` page may only open `wss:`,
  so a page served over HTTPS reaches a server through a TLS proxy (Caddy,
  nginx) in front of its port.
- **Sounds kept in memory**: every sound file fetched stays in the virtual
  filesystem for the session, as do the decoded samples' 64 MB cache.
- **Controller output**: rumble through gilrs' force feedback has no thread
  on the web, and the DualSense's own output needs HID.
- **Mods** that run WebAssembly through wasmtime can't run inside the page.
