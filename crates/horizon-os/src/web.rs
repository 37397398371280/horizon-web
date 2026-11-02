//! The page's side of the game: what the page calls before and to start the
//! game, and where the log and panics go (the browser's console).

use std::sync::Mutex;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function horizon_download(name, bytes) {
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([bytes.slice()]));
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 60000);
}
export function horizon_clipboard_image(png) {
    try {
        const item = new ClipboardItem({ "image/png": new Blob([png.slice()], { type: "image/png" }) });
        navigator.clipboard.write([item]).catch(() => {});
        return true;
    } catch (e) {
        return false;
    }
}
let recorder = null;
let recordState = 0;
let recordError = "";
let recordStart = 0;
export function horizon_recorder_supported() {
    return !!(globalThis.MediaRecorder && HTMLCanvasElement.prototype.captureStream);
}
export function horizon_recorder_state() {
    return recordState;
}
export function horizon_recorder_error() {
    return recordError;
}
export function horizon_recorder_elapsed_ms() {
    return recordState === 2 ? performance.now() - recordStart : 0;
}
export function horizon_recorder_stop() {
    if (recorder && recorder.state !== "inactive") recorder.stop();
}
export function horizon_recorder_start(fps, kbps, gameVolume, mic, micVolume, name) {
    if (recordState !== 0) return;
    recordState = 1;
    recordError = "";
    (async () => {
        const canvas = document.querySelector("canvas");
        const video = canvas.captureStream(fps);
        const tracks = [...video.getVideoTracks()];
        const master = (globalThis.horizonAudioMasters || [])[0];
        let micStream = null;
        if (master) {
            const ctx = master.context;
            const out = ctx.createMediaStreamDestination();
            const game = ctx.createGain();
            game.gain.value = gameVolume;
            master.connect(game);
            game.connect(out);
            if (mic) {
                try {
                    micStream = await navigator.mediaDevices.getUserMedia({ audio: true });
                    const source = ctx.createMediaStreamSource(micStream);
                    const gain = ctx.createGain();
                    gain.gain.value = micVolume;
                    source.connect(gain);
                    gain.connect(out);
                } catch (e) {
                    recordError = "The microphone wasn't allowed: recording without it.";
                }
            }
            tracks.push(...out.stream.getAudioTracks());
        }
        const types = ["video/mp4;codecs=avc1,mp4a", "video/mp4", "video/webm;codecs=vp9,opus", "video/webm"];
        const type = types.find((t) => MediaRecorder.isTypeSupported(t)) || "";
        const chunks = [];
        recorder = new MediaRecorder(new MediaStream(tracks), { mimeType: type, videoBitsPerSecond: kbps * 1000 });
        recorder.ondataavailable = (e) => { if (e.data.size) chunks.push(e.data); };
        recorder.onstop = () => {
            const ext = (recorder.mimeType || type).startsWith("video/mp4") ? ".mp4" : ".webm";
            const a = document.createElement("a");
            a.href = URL.createObjectURL(new Blob(chunks, { type: recorder.mimeType || type }));
            a.download = name + ext;
            a.click();
            setTimeout(() => URL.revokeObjectURL(a.href), 60000);
            if (micStream) micStream.getTracks().forEach((t) => t.stop());
            video.getTracks().forEach((t) => t.stop());
            recorder = null;
            recordState = 0;
        };
        recorder.start(1000);
        recordStart = performance.now();
        recordState = 2;
    })().catch((e) => {
        recordError = "Recording didn't start: " + e;
        recordState = 0;
    });
}
export function horizon_utc_offset_ms() {
    return -new Date().getTimezoneOffset() * 60000;
}
export function horizon_open_url(url) {
    window.open(url, "_blank", "noopener,noreferrer");
}
export function horizon_console(level, message) {
    if (level <= 1) console.error(message);
    else if (level === 2) console.warn(message);
    else console.log(message);
}

let wantLock = false;
let lostLock = false;
let hooked = null;
export function horizon_pointer_lock_lost() {
    const lost = lostLock;
    lostLock = false;
    return lost;
}
function lock(canvas) {
    // Refused outside a click or key press: by a rejected promise, or a throw.
    try {
        const p = canvas.requestPointerLock();
        if (p && p.catch) p.catch(() => {});
    } catch (e) {}
}
export function horizon_pointer_locked() {
    const canvas = document.querySelector("canvas");
    return !!canvas && document.pointerLockElement === canvas;
}
export function horizon_pointer_lock(want) {
    wantLock = want;
    const canvas = document.querySelector("canvas");
    if (!canvas) return;
    if (hooked === null) {
        // Escape while the pointer is locked only releases the lock: the page
        // never sees the key, so the game hears of it as a lost lock.
        document.addEventListener("pointerlockchange", () => {
            if (wantLock && !document.pointerLockElement) lostLock = true;
        });
    }
    if (hooked !== canvas) {
        hooked = canvas;
        // Browsers refuse the lock asked with Escape (no user gesture), and for
        // a moment after Escape let it go: the game then waits for the next
        // click or key. The click that takes the lock only takes the lock, so
        // it doesn't swing a sword.
        let swallowUp = false;
        window.addEventListener("pointerdown", (e) => {
            if (wantLock && e.target === canvas && document.pointerLockElement !== canvas) {
                lock(canvas);
                swallowUp = true;
                e.stopImmediatePropagation();
            }
        }, true);
        window.addEventListener("pointerup", (e) => {
            if (swallowUp) {
                swallowUp = false;
                e.stopImmediatePropagation();
            }
        }, true);
        window.addEventListener("keydown", (e) => {
            if (wantLock && e.key !== "Escape" && document.pointerLockElement !== canvas) lock(canvas);
        }, true);
    }
    if (want) {
        if (document.pointerLockElement !== canvas) lock(canvas);
    } else if (document.pointerLockElement) {
        document.exitPointerLock();
    }
}

let picked = null;
export function horizon_pick_file(kind, accept) {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.onchange = async () => {
        const file = input.files && input.files[0];
        if (file) picked = [kind, new Uint8Array(await file.arrayBuffer())];
    };
    input.click();
}
export function horizon_take_picked() {
    const p = picked;
    picked = null;
    return p;
}
"#)]
extern "C" {
    fn horizon_download(name: &str, bytes: &[u8]);
    fn horizon_clipboard_image(png: &[u8]) -> bool;
    fn horizon_open_url(url: &str);
    fn horizon_utc_offset_ms() -> f64;
    fn horizon_recorder_supported() -> bool;
    fn horizon_recorder_state() -> u32;
    fn horizon_recorder_error() -> String;
    fn horizon_recorder_elapsed_ms() -> f64;
    fn horizon_recorder_stop();
    fn horizon_recorder_start(
        fps: u32,
        kbps: u32,
        game_volume: f32,
        mic: bool,
        mic_volume: f32,
        name: &str,
    );
    fn horizon_pick_file(kind: &str, accept: &str);
    fn horizon_take_picked() -> JsValue;
    fn horizon_console(level: u32, message: &str);
    fn horizon_pointer_lock(want: bool);
    fn horizon_pointer_lock_lost() -> bool;
    fn horizon_pointer_locked() -> bool;
}

static START: Mutex<Option<fn()>> = Mutex::new(None);
static ARGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Called from `main`, which wasm-bindgen runs as soon as the module loads:
/// the game itself starts once the page has installed its files and calls
/// `horizon_start`.
pub fn on_start(start: fn()) {
    *START.lock().unwrap_or_else(|e| e.into_inner()) = Some(start);
}

/// Adds the files of a tar archive under `root` in the virtual filesystem.
#[wasm_bindgen]
pub fn horizon_install_tar(root: &str, tar: &[u8]) -> Result<usize, JsValue> {
    crate::fs::install_tar(root, tar).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Lists files under `root` to fetch from `base` when first read (one
/// `path<TAB>length` per line).
#[wasm_bindgen]
pub fn horizon_install_manifest(root: &str, base: &str, manifest: &str) {
    crate::fs::install_manifest(root, base, manifest);
}

/// A file the page kept in the browser's storage, back in the virtual
/// filesystem before the game starts (`modified` in ms since 1970).
#[wasm_bindgen]
pub fn horizon_install_saved_file(path: &str, data: Vec<u8>, modified: f64) {
    crate::fs::install_saved_file(path, data, modified);
}

/// A folder the page kept in the browser's storage.
#[wasm_bindgen]
pub fn horizon_install_saved_dir(path: &str) {
    crate::fs::install_saved_dir(path);
}

/// The player's files changed since the last call, for the page to save:
/// `[path, "file", bytes]`, `[path, "dir" | "cleardir"]` (emptied first) or
/// `[path, "gone"]`, parents before children.
#[wasm_bindgen]
pub fn horizon_take_changes() -> js_sys::Array {
    let changes = js_sys::Array::new();
    for (path, change) in crate::fs::take_changes() {
        let entry = js_sys::Array::new();
        entry.push(&JsValue::from_str(&path));
        match change {
            crate::fs::Change::File(data) => {
                entry.push(&JsValue::from_str("file"));
                entry.push(&js_sys::Uint8Array::from(&data[..]));
            }
            crate::fs::Change::Dir { clear } => {
                entry.push(&JsValue::from_str(if clear { "cleardir" } else { "dir" }));
            }
            crate::fs::Change::Gone => {
                entry.push(&JsValue::from_str("gone"));
            }
        }
        changes.push(&entry);
    }
    changes
}

/// Starts the game registered by [`on_start`] with these arguments (what
/// native takes on its command line).
#[wasm_bindgen]
pub fn horizon_start(args: Vec<String>) {
    *ARGS.lock().unwrap_or_else(|e| e.into_inner()) = args;
    std::panic::set_hook(Box::new(|info| {
        horizon_console(1, &format!("Horizon panicked: {info}"));
    }));
    #[cfg(target_feature = "atomics")]
    crate::net::install();
    let start = START.lock().unwrap_or_else(|e| e.into_inner()).take();
    match start {
        Some(start) => start(),
        None => horizon_console(1, "Horizon: no game registered to start"),
    }
}

/// The arguments the page passed to [`horizon_start`].
pub(crate) fn args() -> Vec<String> {
    ARGS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Hands `bytes` to the browser as a download called `name`.
pub fn download(name: &str, bytes: &[u8]) {
    horizon_download(name, bytes);
}

/// Puts a PNG on the clipboard as an image; false where the browser can't.
pub fn clipboard_image(png: &[u8]) -> bool {
    horizon_clipboard_image(png)
}

/// The local time zone's offset from UTC, in milliseconds.
pub fn utc_offset_ms() -> i64 {
    horizon_utc_offset_ms() as i64
}

/// Whether the page can record the canvas (`MediaRecorder`, `captureStream`).
pub fn recorder_supported() -> bool {
    horizon_recorder_supported()
}

/// The page's recording: 0 none, 1 starting (asking for the microphone), 2 recording.
pub fn recorder_state() -> u32 {
    horizon_recorder_state()
}

/// Why the last recording failed or went without the microphone ("" for nothing).
pub fn recorder_error() -> String {
    horizon_recorder_error()
}

/// How long the recording has gone on, in milliseconds.
pub fn recorder_elapsed_ms() -> u64 {
    horizon_recorder_elapsed_ms() as u64
}

/// Records the canvas, the game's sound at `game_volume` and, with `mic`, the microphone at
/// `mic_volume`; when stopped, the video downloads as `name` with its format's extension.
pub fn recorder_start(
    fps: u32,
    kbps: u32,
    game_volume: f32,
    mic: bool,
    mic_volume: f32,
    name: &str,
) {
    horizon_recorder_start(fps, kbps, game_volume, mic, mic_volume, name);
}

/// Stops the recording, which then downloads.
pub fn recorder_stop() {
    horizon_recorder_stop();
}

/// Opens `url` in a new tab.
pub fn open_url(url: &str) {
    horizon_open_url(url);
}

/// A line in the browser's console (`log::Level` as a number: 1 error, 2
/// warning, more for the rest).
pub fn console(level: u32, message: &str) {
    horizon_console(level, message);
}

/// Locks the pointer to the game's canvas, or lets it go. Browsers lock it
/// only during a click or a key press: refused now, the canvas takes it on
/// the player's next click.
pub fn pointer_lock(want: bool) {
    horizon_pointer_lock(want);
}

/// Whether the browser holds the pointer for the game's canvas now. Until it
/// does, the pointer moves freely and its motion must not turn the view.
pub fn pointer_locked() -> bool {
    horizon_pointer_locked()
}

/// Whether the browser let the pointer go while the game held it (the
/// player pressed Escape, which the page never sees then) since the last call.
pub fn pointer_lock_lost() -> bool {
    horizon_pointer_lock_lost()
}

/// Asks the player for a file (`accept`: the file chooser's types). Browsers
/// open the chooser only during a click or a key press; the file comes later
/// from [`take_picked_file`], with `kind` to tell requests apart.
pub fn pick_file(kind: &str, accept: &str) {
    horizon_pick_file(kind, accept);
}

/// The file the player picked since the last call: its `kind` and bytes.
pub fn take_picked_file() -> Option<(String, Vec<u8>)> {
    let picked = horizon_take_picked();
    if picked.is_null() {
        return None;
    }
    let picked = js_sys::Array::from(&picked);
    let kind = picked.get(0).as_string()?;
    Some((kind, js_sys::Uint8Array::new(&picked.get(1)).to_vec()))
}
