//! F then a digit types a function key (F+3 is F3, F+0 is F10), for keyboards with no
//! function row the page can read: on ChromeOS only (`FChord::for_this_device`), as the Java
//! build's `DomEventQueue.chord`. Elsewhere F goes to the game the moment it's pressed, so
//! swap hands and anything held on F work as they do on the desktop.
//!
//! F alone still presses F, but only when it's released or another key goes down, because
//! nothing can tell a digit is coming. A chord's F is never reported, so it doesn't also act
//! as its own key (swap hands). Not while a text box has focus, where "f3" is text.
use winit::event::ElementState;
use winit::keyboard::{KeyCode, NamedKey};

/// What the game should do with one key event.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Verdict {
    /// Report the F that was held back, as a press, before this event.
    pub flush_f: bool,
    pub then: Then,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Then {
    /// The event as it came.
    Pass,
    /// Nothing: the chord used it, or it holds F back.
    Swallow,
    /// The event as this function key (`repeat` as it came).
    AsFunction(KeyCode),
    /// Hold this event (F's press) until the next one decides.
    Hold,
}

#[derive(Default)]
pub(super) struct FChord {
    /// Every key passes as it came: not ChromeOS (`for_this_device`).
    off: bool,
    /// F is down and not yet reported: a digit may still turn it into a function key.
    pending: bool,
    /// F is down and a digit already turned it into a function key; its release isn't reported.
    used: bool,
    /// The function key each held digit stands for, so its release releases that key.
    digits: [Option<KeyCode>; 10],
}

impl FChord {
    /// On in the browser on ChromeOS (or as `?fchord=` says), off everywhere else.
    pub fn for_this_device() -> Self {
        #[cfg(target_arch = "wasm32")]
        let on = horizon_os::web::function_key_chords();
        #[cfg(not(target_arch = "wasm32"))]
        let on = false;
        Self {
            off: !on,
            ..Self::default()
        }
    }

    /// Forget everything held: focus left, and the game releases its own keys.
    pub fn reset(&mut self) {
        *self = Self {
            off: self.off,
            ..Self::default()
        };
    }

    /// `blocked` is Control, Alt or Super held; `typing` is a text box having focus.
    pub fn feed(
        &mut self,
        key: KeyCode,
        state: ElementState,
        repeat: bool,
        blocked: bool,
        typing: bool,
    ) -> Verdict {
        let pressed = state == ElementState::Pressed;
        let pass = |flush_f| Verdict {
            flush_f,
            then: Then::Pass,
        };
        if self.off {
            return pass(false);
        }
        if key == KeyCode::KeyF {
            if pressed {
                if !repeat && !blocked && !typing {
                    self.pending = true;
                    return Verdict {
                        flush_f: false,
                        then: Then::Hold,
                    };
                }
                // A repeat of an F the chord holds, or an F the game already saw.
                let swallowed = repeat && (self.pending || self.used);
                return Verdict {
                    flush_f: false,
                    then: if swallowed { Then::Swallow } else { Then::Pass },
                };
            }
            if self.pending {
                self.pending = false;
                return pass(true);
            }
            if self.used {
                self.used = false;
                return Verdict {
                    flush_f: false,
                    then: Then::Swallow,
                };
            }
            return pass(false);
        }
        let slot = digit_slot(key);
        if !pressed {
            if let Some(slot) = slot
                && let Some(function) = self.digits[slot].take()
            {
                return Verdict {
                    flush_f: false,
                    then: Then::AsFunction(function),
                };
            }
            return pass(false);
        }
        if let Some(slot) = slot
            && !blocked
            && !typing
            && (self.pending || self.used)
        {
            if !repeat {
                self.pending = false;
                self.used = true;
                self.digits[slot] = Some(function_key(slot));
            }
            return match self.digits[slot] {
                Some(function) => Verdict {
                    flush_f: false,
                    then: Then::AsFunction(function),
                },
                None => Verdict {
                    flush_f: false,
                    then: Then::Swallow,
                },
            };
        }
        // Not a chord after all: the F goes first, then this key. Modifiers don't end the
        // wait, so F+Shift+3 works.
        let flush = self.pending && !is_modifier(key);
        if flush {
            self.pending = false;
        }
        pass(flush)
    }
}

/// 0..=9 for the keys 1..9 then 0.
fn digit_slot(key: KeyCode) -> Option<usize> {
    Some(match key {
        KeyCode::Digit1 => 0,
        KeyCode::Digit2 => 1,
        KeyCode::Digit3 => 2,
        KeyCode::Digit4 => 3,
        KeyCode::Digit5 => 4,
        KeyCode::Digit6 => 5,
        KeyCode::Digit7 => 6,
        KeyCode::Digit8 => 7,
        KeyCode::Digit9 => 8,
        KeyCode::Digit0 => 9,
        _ => return None,
    })
}

fn function_key(slot: usize) -> KeyCode {
    [
        KeyCode::F1,
        KeyCode::F2,
        KeyCode::F3,
        KeyCode::F4,
        KeyCode::F5,
        KeyCode::F6,
        KeyCode::F7,
        KeyCode::F8,
        KeyCode::F9,
        KeyCode::F10,
    ][slot]
}

/// The logical key a function key's event carries.
pub(super) fn named(key: KeyCode) -> Option<NamedKey> {
    Some(match key {
        KeyCode::F1 => NamedKey::F1,
        KeyCode::F2 => NamedKey::F2,
        KeyCode::F3 => NamedKey::F3,
        KeyCode::F4 => NamedKey::F4,
        KeyCode::F5 => NamedKey::F5,
        KeyCode::F6 => NamedKey::F6,
        KeyCode::F7 => NamedKey::F7,
        KeyCode::F8 => NamedKey::F8,
        KeyCode::F9 => NamedKey::F9,
        KeyCode::F10 => NamedKey::F10,
        _ => return None,
    })
}

fn is_modifier(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ElementState::{Pressed, Released};

    fn press(c: &mut FChord, key: KeyCode) -> Verdict {
        c.feed(key, Pressed, false, false, false)
    }

    fn release(c: &mut FChord, key: KeyCode) -> Verdict {
        c.feed(key, Released, false, false, false)
    }

    fn v(flush_f: bool, then: Then) -> Verdict {
        Verdict { flush_f, then }
    }

    #[test]
    fn off_passes_f_at_once() {
        let mut c = FChord {
            off: true,
            ..FChord::default()
        };
        assert_eq!(press(&mut c, KeyCode::KeyF), v(false, Then::Pass));
        assert_eq!(press(&mut c, KeyCode::Digit3), v(false, Then::Pass));
        assert_eq!(release(&mut c, KeyCode::Digit3), v(false, Then::Pass));
        assert_eq!(release(&mut c, KeyCode::KeyF), v(false, Then::Pass));
        c.reset();
        assert_eq!(press(&mut c, KeyCode::KeyF), v(false, Then::Pass));
    }

    #[test]
    fn off_on_the_desktop() {
        let mut c = FChord::for_this_device();
        assert_eq!(press(&mut c, KeyCode::KeyF), v(false, Then::Pass));
    }

    #[test]
    fn f_then_digit_is_a_function_key() {
        let mut c = FChord::default();
        assert_eq!(press(&mut c, KeyCode::KeyF), v(false, Then::Hold));
        assert_eq!(
            press(&mut c, KeyCode::Digit3),
            v(false, Then::AsFunction(KeyCode::F3))
        );
        assert_eq!(
            release(&mut c, KeyCode::Digit3),
            v(false, Then::AsFunction(KeyCode::F3))
        );
        assert_eq!(release(&mut c, KeyCode::KeyF), v(false, Then::Swallow));
    }

    #[test]
    fn the_function_key_stays_held_for_a_third_key() {
        let mut c = FChord::default();
        press(&mut c, KeyCode::KeyF);
        press(&mut c, KeyCode::Digit3);
        assert_eq!(press(&mut c, KeyCode::KeyH), v(false, Then::Pass));
        assert_eq!(release(&mut c, KeyCode::KeyH), v(false, Then::Pass));
        assert_eq!(
            release(&mut c, KeyCode::Digit3),
            v(false, Then::AsFunction(KeyCode::F3))
        );
    }

    #[test]
    fn zero_is_f10_and_a_second_digit_joins() {
        let mut c = FChord::default();
        press(&mut c, KeyCode::KeyF);
        assert_eq!(
            press(&mut c, KeyCode::Digit0),
            v(false, Then::AsFunction(KeyCode::F10))
        );
        assert_eq!(
            press(&mut c, KeyCode::Digit5),
            v(false, Then::AsFunction(KeyCode::F5))
        );
    }

    #[test]
    fn digit_repeats_repeat_the_function_key() {
        let mut c = FChord::default();
        press(&mut c, KeyCode::KeyF);
        press(&mut c, KeyCode::Digit3);
        assert_eq!(
            c.feed(KeyCode::Digit3, Pressed, true, false, false),
            v(false, Then::AsFunction(KeyCode::F3))
        );
        assert_eq!(
            c.feed(KeyCode::KeyF, Pressed, true, false, false),
            v(false, Then::Swallow)
        );
    }

    #[test]
    fn f_alone_is_reported_on_release() {
        let mut c = FChord::default();
        assert_eq!(press(&mut c, KeyCode::KeyF), v(false, Then::Hold));
        assert_eq!(release(&mut c, KeyCode::KeyF), v(true, Then::Pass));
    }

    #[test]
    fn f_comes_first_when_another_key_follows() {
        let mut c = FChord::default();
        press(&mut c, KeyCode::KeyF);
        assert_eq!(press(&mut c, KeyCode::KeyW), v(true, Then::Pass));
        assert_eq!(release(&mut c, KeyCode::KeyF), v(false, Then::Pass));
        // F is out now: a digit is a digit.
        assert_eq!(press(&mut c, KeyCode::Digit3), v(false, Then::Pass));
    }

    #[test]
    fn shift_does_not_end_the_wait() {
        let mut c = FChord::default();
        press(&mut c, KeyCode::KeyF);
        assert_eq!(press(&mut c, KeyCode::ShiftLeft), v(false, Then::Pass));
        assert_eq!(
            press(&mut c, KeyCode::Digit3),
            v(false, Then::AsFunction(KeyCode::F3))
        );
    }

    #[test]
    fn digits_alone_are_digits() {
        let mut c = FChord::default();
        assert_eq!(press(&mut c, KeyCode::Digit3), v(false, Then::Pass));
        assert_eq!(release(&mut c, KeyCode::Digit3), v(false, Then::Pass));
    }

    #[test]
    fn no_chord_while_typing_or_with_ctrl() {
        let mut c = FChord::default();
        assert_eq!(
            c.feed(KeyCode::KeyF, Pressed, false, false, true),
            v(false, Then::Pass)
        );
        assert_eq!(
            c.feed(KeyCode::Digit3, Pressed, false, false, true),
            v(false, Then::Pass)
        );
        let mut c = FChord::default();
        assert_eq!(
            c.feed(KeyCode::KeyF, Pressed, false, true, false),
            v(false, Then::Pass)
        );
    }

    #[test]
    fn reset_forgets_a_held_f() {
        let mut c = FChord::default();
        press(&mut c, KeyCode::KeyF);
        c.reset();
        assert_eq!(release(&mut c, KeyCode::KeyF), v(false, Then::Pass));
    }
}
