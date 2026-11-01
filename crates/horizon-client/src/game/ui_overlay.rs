//! The game's side of the overlay hooks (`native_screen_claim`,
//! `native_overlay`, `native_overlay_input`) and the helpers that let a
//! mod's own menus drive the game's (`native_open`, `native_connect`): what
//! a client built on Horizon needs to replace the title and pause screens
//! with its own.
use super::{Game, Screen};
use horizon_client::title::{Action, Mode};
use std::cell::RefCell;

thread_local! {
    /// Title actions a mod asked for (`native_open`), run where the title's
    /// own clicks run: some need the event loop (Quit).
    static QUEUED: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
}

impl Game<'_> {
    /// `native_ui_screen`'s id for what shows now.
    pub(super) fn ui_screen_id(&self) -> &'static str {
        if let Some(title) = &self.title {
            if self.screen == Screen::Options {
                return "options";
            }
            return match &title.mode {
                Mode::Main => "title",
                Mode::Select { .. } => "title_worlds",
                Mode::Create { .. } | Mode::CreateFlat { .. } => "title_create",
                Mode::Multiplayer(_) | Mode::MultiplayerWarning { .. } => "title_multiplayer",
                _ => "title_other",
            };
        }
        if self.remote_loading() || self.loading_reason().is_some() {
            return "loading";
        }
        self.screen.id()
    }
    /// Whether a mod draws the screen showing now itself.
    pub(super) fn ui_screen_claimed(&self) -> bool {
        let mut claimed = false;
        self.native_screen_claim(self.ui_screen_id(), &mut claimed);
        claimed
    }
    /// `native_open`.
    pub(super) fn open_ui(&mut self, what: &str) -> bool {
        if let Some(title) = self.title.as_mut() {
            let id = match what {
                "singleplayer" => 1,
                "quit" => 2,
                "options" => 3,
                "multiplayer" => 4,
                "language" => 6,
                "accessibility" => 7,
                "profile" => 8,
                "credits" => {
                    QUEUED.with(|q| q.borrow_mut().push(Action::Credits));
                    return true;
                }
                "packs" => {
                    self.open_options_at(true, horizon_client::options_ui::Page::Packs);
                    return true;
                }
                "title" => {
                    title.mode = Mode::Main;
                    return true;
                }
                _ => return false,
            };
            let action = title.press_main(id);
            if action != Action::None {
                QUEUED.with(|q| q.borrow_mut().push(action));
            }
            return true;
        }
        if self.player.is_none() {
            return false;
        }
        let pause_action = match what {
            "resume" => 0,
            "options" => 1,
            "disconnect" => 2,
            "stats" => 3,
            "advancements" => 4,
            "lan" => 9,
            "friends" => 10,
            "packs" => {
                self.open_options_at(false, horizon_client::options_ui::Page::Packs);
                return true;
            }
            "pause" => {
                if self.screen == Screen::Mod {
                    self.close_mod_screen();
                }
                self.pause_focus = None;
                self.screen = Screen::Paused;
                self.pause_menu = true;
                self.capture(false);
                return true;
            }
            _ => return false,
        };
        if let Err(e) = self.execute_pause_action(pause_action) {
            log::warn!("Horizon: {what}: {e}");
            return false;
        }
        true
    }
    /// `native_connect`.
    pub(super) fn connect_from_title(&mut self, address: &str) -> bool {
        if self.title.is_none() {
            return false;
        }
        QUEUED.with(|q| {
            q.borrow_mut().push(Action::Connect {
                address: address.to_owned(),
                lan: false,
            })
        });
        true
    }
    /// The title actions mods queued, run now.
    pub(super) fn run_queued_ui(&mut self, event_loop: &dyn super::event_loop::EventLoopCtl) {
        let queued = QUEUED.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for action in queued {
            if let Err(e) = self.title_action(event_loop, action) {
                self.error = Some(e);
            }
        }
    }
    /// `native_gui_scale`.
    pub(super) fn ui_gui_scale(&self) -> f64 {
        let size = self
            .renderer
            .as_ref()
            .map_or([1280, 720], |r| [r.config.width, r.config.height]);
        let b = horizon_client::hud::HudBatch::new(size);
        f64::from(b.scale)
    }
}

impl Game<'_> {
    /// `native_overlay_input` for a window event, before the game handles
    /// it: true when the overlay took it. A key's release always goes on to
    /// the game too, so nothing it holds sticks.
    pub(super) fn overlay_input_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        use super::native_hooks::NativeInput;
        use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
        use winit::keyboard::PhysicalKey;
        let at = [self.cursor.0, self.cursor.1];
        let modifiers = super::glfw_modifiers(self.modifiers);
        let screen = self.ui_screen_id();
        let mut consumed = false;
        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                let input = NativeInput::Button {
                    button: super::glfw_button(*button),
                    pressed: *state == ElementState::Pressed,
                    at,
                    modifiers,
                };
                self.native_overlay_input(screen, &input, &mut consumed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (*x, *y),
                    MouseScrollDelta::PixelDelta(p) => ((p.x / 30.0) as f32, (p.y / 30.0) as f32),
                };
                let input = NativeInput::Scroll { dx, dy, at };
                self.native_overlay_input(screen, &input, &mut consumed);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key
                    && let Some(name) = horizon_client_input::keybinds::key_name(code)
                {
                    let input = NativeInput::Key {
                        name,
                        pressed,
                        repeat: event.repeat,
                        modifiers,
                    };
                    self.native_overlay_input(screen, &input, &mut consumed);
                }
                // Typed text, unless the key itself was taken or Control or
                // Super makes it a shortcut.
                if pressed
                    && !consumed
                    && modifiers & (2 | 8) == 0
                    && let Some(text) = event.text.as_deref()
                    && text.chars().all(|c| !c.is_control())
                {
                    self.native_overlay_input(screen, &NativeInput::Text { text }, &mut consumed);
                }
                if !pressed {
                    consumed = false;
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
                self.native_overlay_input(screen, &NativeInput::Text { text }, &mut consumed);
            }
            _ => {}
        }
        consumed
    }
    /// The pointer moved: `native_overlay_input` hears where it is now.
    pub(super) fn overlay_pointer_moved(&mut self) {
        let at = [self.cursor.0, self.cursor.1];
        let screen = self.ui_screen_id();
        let mut consumed = false;
        self.native_overlay_input(
            screen,
            &super::native_hooks::NativeInput::Move { at },
            &mut consumed,
        );
    }
}
