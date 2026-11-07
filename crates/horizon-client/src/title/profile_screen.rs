//! Edit Profile (Eaglercraft's GuiScreenEditProfile), the browser build's
//! first screen and the title screen's Edit Profile button: the offline name,
//! the skin (a default one or an imported picture), its arms, an imported
//! cape, and the player drawn in them.
use super::{Button, Mode, Title, centered};
use crate::{
    hud::{HudAtlas, HudBatch, rgba},
    profile::{self, CHARACTERS, Profile, Skin},
};
use std::cell::RefCell;

pub(super) const NAME: crate::clipboard::FieldId = ("profile.name", 0);

/// Button ids.
const PREVIOUS: u8 = 80;
const SKIN: u8 = 81;
const NEXT: u8 = 82;
const ARMS: u8 = 83;
const IMPORT_SKIN: u8 = 84;
const CAPE: u8 = 85;
const DONE: u8 = 86;
const CANCEL: u8 = 87;

#[derive(Clone, Debug)]
pub struct State {
    pub profile: Profile,
    pub focused: bool,
    /// An imported skin is there to pick.
    custom: bool,
    /// The first profile: no Cancel.
    first: bool,
    message: Option<String>,
    /// The drawn skin's pixels, for the look they were read for.
    preview: RefCell<Option<((Skin, bool), Option<Vec<u8>>)>>,
}

fn dir() -> std::path::PathBuf {
    horizon_os::env::current_dir().unwrap_or_default()
}

impl State {
    pub(super) fn new(first: bool) -> Self {
        let dir = dir();
        let profile = profile::load(&dir).unwrap_or_else(|| Profile::new(&dir));
        let custom = profile.skin == Skin::Custom
            || horizon_os::fs::metadata(dir.join("horizon-skin.png")).is_ok();
        Self {
            profile,
            focused: first,
            custom,
            first,
            message: None,
            preview: RefCell::new(None),
        }
    }

    pub(super) fn is_first(&self) -> bool {
        self.first
    }

    /// The skin choices: the default characters, then the imported skin.
    fn choices(&self) -> usize {
        CHARACTERS.len() + usize::from(self.custom)
    }

    fn choice(&self) -> usize {
        match self.profile.skin {
            Skin::Default(i) => i,
            Skin::Custom => CHARACTERS.len(),
        }
    }

    fn set_choice(&mut self, i: usize) {
        self.profile.skin = if i >= CHARACTERS.len() {
            Skin::Custom
        } else {
            // Alex is the one character slim by default.
            self.profile.slim = i == 1;
            Skin::Default(i)
        };
    }

    fn skin_label(&self) -> String {
        match self.profile.skin {
            Skin::Default(i) => {
                let name = CHARACTERS[i];
                name[..1].to_uppercase() + &name[1..]
            }
            Skin::Custom => "Imported Skin".into(),
        }
    }
}

/// Where the controls start: a column right of the preview.
fn column(w: f32) -> f32 {
    (w.ceil() / 2.0).floor() - 40.0
}

pub(super) fn buttons(state: &State, w: f32, h: f32) -> Vec<Button> {
    let x = column(w);
    let b = |x: f32, y: f32, w: f32, label: &str, enabled: bool, id: u8| Button {
        x,
        y,
        w,
        label: label.into(),
        enabled,
        id,
        icon: None,
        trailing: None,
        action: None,
    };
    let arms = if state.profile.slim {
        "Arms: Slim"
    } else {
        "Arms: Classic"
    };
    let cape = if state.profile.cape {
        "Remove Cape"
    } else {
        "Import Cape..."
    };
    let valid = crate::offline_name::is_valid(&state.profile.name);
    let (h, cx) = (h.ceil(), (w.ceil() / 2.0).floor());
    let mut buttons = vec![
        b(x, 92.0, 20.0, "<", true, PREVIOUS),
        b(x + 22.0, 92.0, 156.0, &state.skin_label(), true, SKIN),
        b(x + 180.0, 92.0, 20.0, ">", true, NEXT),
        b(x, 116.0, 200.0, arms, true, ARMS),
        b(x, 140.0, 98.0, "Import Skin...", true, IMPORT_SKIN),
        b(x + 102.0, 140.0, 98.0, cape, true, CAPE),
    ];
    if state.first {
        buttons.push(b(cx - 100.0, h - 28.0, 200.0, "Done", valid, DONE));
    } else {
        let done = crate::lang::get_or("gui.done", "Done");
        let cancel = crate::lang::get_or("gui.cancel", "Cancel");
        buttons.push(b(cx - 154.0, h - 28.0, 150.0, &done, valid, DONE));
        buttons.push(b(cx + 4.0, h - 28.0, 150.0, &cancel, true, CANCEL));
    }
    buttons
}

/// The name's box.
pub(super) fn name_rect(w: f32) -> [f32; 4] {
    [column(w), 52.0, 200.0, 20.0]
}

/// A part of the skin as the front view shows it: where it is in the skin
/// (x, y, width, height), and where it goes in the 16×32 figure.
type Part = ([u32; 4], [u32; 2]);

/// The front of each part, then its outer layer.
fn front(slim: bool) -> Vec<Part> {
    let arm = if slim { 3 } else { 4 };
    let mut parts = Vec::new();
    for outer in [false, true] {
        // The outer layers: the head's 32 to the right, the right limbs'
        // and the body's 16 down, the left limbs' beside the inner ones.
        let dy = if outer { 16 } else { 0 };
        parts.push(([if outer { 40 } else { 8 }, 8, 8, 8], [4, 0]));
        parts.push(([20, 20 + dy, 8, 12], [4, 8]));
        parts.push(([44, 20 + dy, arm, 12], [4 - arm, 8]));
        parts.push(([if outer { 52 } else { 36 }, 52, arm, 12], [12, 8]));
        parts.push(([4, 20 + dy, 4, 12], [4, 20]));
        parts.push(([if outer { 4 } else { 20 }, 52, 4, 12], [8, 20]));
    }
    parts
}

pub(super) fn draw(
    title: &Title,
    state: &State,
    atlas: &HudAtlas,
    batch: &mut HudBatch,
    cursor: (f32, f32),
) {
    let (w, h) = (batch.width.ceil(), batch.height.ceil());
    crate::ui::menu_background(batch, atlas, 0.0, 0.0, w, h, false);
    centered(atlas, batch, "Edit Profile", 15.0, u32::MAX);
    let x = column(w);
    let gray = rgba(160, 160, 160, 255);
    batch.text(atlas, "Username", x, 40.0, gray, true);
    let field = name_rect(w);
    title.caret.borrow_mut().draw(
        batch,
        atlas,
        (NAME, &state.profile.name, 16),
        field,
        state.focused,
        title.started,
    );
    batch.text(atlas, "Skin", x, 80.0, gray, true);
    if let Some(message) = &state.message {
        for (i, line) in super::wrapped(atlas, message, 200.0).iter().enumerate() {
            batch.text(atlas, line, x, 166.0 + i as f32 * 10.0, gray, true);
        }
    }
    // The player, four GUI pixels a skin pixel, left of the controls.
    let key = (state.profile.skin.clone(), state.profile.slim);
    let mut preview = state.preview.borrow_mut();
    if preview.as_ref().is_none_or(|(k, _)| *k != key) {
        let pixels = profile::skin_pixels(&dir(), &title.resources, &state.profile);
        *preview = Some((key, pixels));
    }
    let scale = 4.0;
    let (px, py) = (x - 30.0 - 16.0 * scale, 40.0);
    batch.fill(
        atlas,
        px - 6.0,
        py - 6.0,
        16.0 * scale + 12.0,
        32.0 * scale + 12.0,
        rgba(0, 0, 0, 96),
    );
    if let Some((_, Some(pixels))) = preview.as_ref() {
        for ([sx, sy, sw, sh], [fx, fy]) in front(state.profile.slim) {
            for dy in 0..sh {
                for dx in 0..sw {
                    let at = (((sy + dy) * 64 + sx + dx) * 4) as usize;
                    let Some(&[r, g, b, a]) = pixels.get(at..at + 4).and_then(|p| p.first_chunk())
                    else {
                        continue;
                    };
                    if a < 128 {
                        continue;
                    }
                    batch.fill(
                        atlas,
                        px + (fx + dx) as f32 * scale,
                        py + (fy + dy) as f32 * scale,
                        scale,
                        scale,
                        rgba(r, g, b, 255),
                    );
                }
            }
        }
    }
    let _ = cursor;
}

/// What a press on the screen's buttons or name box does.
pub(super) fn click(title: &mut Title, hit: Option<u8>, (x, y): (f32, f32), w: f32) {
    let Mode::Profile(state) = &mut title.mode else {
        return;
    };
    let n = state.choices();
    match hit {
        Some(PREVIOUS) => state.set_choice((state.choice() + n - 1) % n),
        Some(SKIN | NEXT) => state.set_choice((state.choice() + 1) % n),
        Some(ARMS) => state.profile.slim = !state.profile.slim,
        Some(IMPORT_SKIN) => pick("skin", state),
        Some(CAPE) if state.profile.cape => state.profile.cape = false,
        Some(CAPE) => pick("cape", state),
        Some(DONE) => title.finish_profile(),
        Some(CANCEL) => title.mode = Mode::Main,
        _ => {
            let [fx, fy, fw, fh] = name_rect(w);
            state.focused = crate::ui::hovered((x, y), fx, fy, fw, fh);
        }
    }
}

/// Asks the browser for a picture; [`Title::poll_profile_file`] takes it.
fn pick(kind: &str, state: &mut State) {
    #[cfg(target_arch = "wasm32")]
    {
        horizon_os::web::pick_file(kind, "image/png");
        state.message = None;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = kind;
        state.message = Some("Pictures are imported in the browser build.".into());
    }
}

/// No profile is saved yet: Edit Profile makes the first.
#[cfg(target_arch = "wasm32")]
pub(super) fn first_launch() -> bool {
    profile::load(&dir()).is_none()
}

impl Title {
    /// Opens Edit Profile; `first` when the player has no profile yet.
    pub fn edit_profile(&mut self, first: bool) {
        self.mode = Mode::Profile(Box::new(State::new(first)));
    }

    /// The browser build's start: the saved profile applied, or Edit
    /// Profile to make the first.
    pub fn start_profile(&mut self) {
        let dir = dir();
        match profile::load(&dir) {
            Some(p) => profile::apply(&dir, &self.resources, &p),
            None => self.edit_profile(true),
        }
    }

    /// Done: the profile saved and worn.
    pub(super) fn finish_profile(&mut self) {
        let Mode::Profile(state) = &mut self.mode else {
            return;
        };
        if !crate::offline_name::is_valid(&state.profile.name) {
            return;
        }
        let dir = dir();
        if let Err(e) = profile::save(&dir, &state.profile) {
            state.message = Some(format!("Could not save the profile: {e}"));
            return;
        }
        profile::apply(&dir, &self.resources, &state.profile);
        self.mode = Mode::Main;
    }

    /// A picture the browser handed over for Import Skin or Import Cape.
    pub(super) fn poll_profile_file(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Mode::Profile(state) = &mut self.mode
            && let Some((kind, png)) = horizon_os::web::take_picked_file()
        {
            let dir = dir();
            let result = if kind == "cape" {
                profile::import_cape(&dir, &png).map(|()| state.profile.cape = true)
            } else {
                profile::import_skin(&dir, &png).map(|()| {
                    state.custom = true;
                    state.profile.skin = Skin::Custom;
                    // A new picture under the same name.
                    *state.preview.get_mut() = None;
                })
            };
            state.message = result.err();
        }
    }
}
