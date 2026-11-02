//! Passwords shown as stars while they're typed (Incari's `HiddenPasswords`): a login plugin's
//! command in chat (`/login`, `/register` and their kin), and a server dialog's text box whose
//! label names a password. Only what's drawn changes; the box holds and sends the text as typed.

/// Login plugins' commands whose arguments are passwords (AuthMe, nLogin, LibreLogin, Eagler's
/// own and their aliases).
const COMMANDS: [&str; 12] = [
    "login",
    "l",
    "log",
    "logar",
    "register",
    "reg",
    "registrar",
    "changepassword",
    "changepass",
    "changepw",
    "unregister",
    "unreg",
];

/// Words a password box's label holds, in the languages login plugins most often speak.
const PASSWORD_WORDS: [&str; 6] = [
    "password",
    "passwort",
    "contraseña",
    "senha",
    "mot de passe",
    "пароль",
];

/// Where a chat line's password starts, in characters: after the command word, if the line is a
/// login command (namespaced or not, any case).
pub fn hidden_from(line: &str) -> Option<usize> {
    let rest = line.strip_prefix('/')?;
    let end = rest.find(' ')?;
    let command = rest[..end].to_lowercase();
    let command = command.rsplit(':').next().unwrap_or_default();
    COMMANDS
        .contains(&command)
        .then(|| rest[..end].chars().count() + 2)
}

/// Whether `line` holds a password, so it's kept out of the chat's history and
/// `command_history.txt`: the stars only hide it while it's typed.
pub fn has_password(line: &str) -> bool {
    hidden_from(line).is_some()
}

pub fn is_password_label(label: &str) -> bool {
    let lower = label.to_lowercase();
    PASSWORD_WORDS.iter().any(|w| lower.contains(w))
}

/// `text`, which starts at character `offset` of the box's value, with every character from
/// `from` on drawn as a star. Spaces stay, so a password and its confirmation show as two.
pub fn starred(text: &str, offset: usize, from: usize) -> String {
    text.chars()
        .enumerate()
        .map(|(i, c)| {
            if offset + i >= from && c != ' ' {
                '*'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_commands_hide_their_arguments() {
        assert_eq!(hidden_from("/login hunter2"), Some(7));
        assert_eq!(hidden_from("/AuthMe:REG a b"), Some(12));
        assert_eq!(hidden_from("/login"), None);
        assert_eq!(hidden_from("/tp a b"), None);
        assert_eq!(hidden_from("login a"), None);
        assert_eq!(starred("/reg ab cd", 0, 5), "/reg ** **");
        assert_eq!(starred("b cd", 6, 5), "* **");
    }

    #[test]
    fn password_labels() {
        assert!(is_password_label("Confirm Password"));
        assert!(is_password_label("Mot de passe"));
        assert!(!is_password_label("Email"));
    }
}
