//! Key bindings.
//!
//! Every shortcut in the tool is a [`Command`] with a binding attached, rather
//! than a `consume_key` call buried in whichever function happened to need it.
//! That is what makes the rest possible: the menus can print the real binding
//! instead of a hardcoded string, the settings window can list every shortcut
//! that exists, and a rebinding can be checked against the others before it is
//! accepted.

use eframe::egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// When a binding is live.
///
/// A single-letter shortcut cannot fire while a text box has focus, or typing
/// a name would rename something instead. Rather than testing that at each
/// call site, every command says what it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// Always, even with a dialog open.
    Global,
    /// Only when nothing is taking typed input.
    Listing,
    /// Only in a listing, with a row selected.
    Selection,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Global => "Anywhere",
            Scope::Listing => "In a listing",
            Scope::Selection => "With a line selected",
        }
    }
}

macro_rules! commands {
    ($(($variant:ident, $id:literal, $label:literal, $scope:expr, $mods:expr, $key:expr)),* $(,)?) => {
        /// Everything that can be bound to a key.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Command {
            $($variant),*
        }

        impl Command {
            pub const ALL: &'static [Command] = &[$(Command::$variant),*];

            /// The name used in the settings file, which must stay stable.
            pub fn id(self) -> &'static str {
                match self { $(Command::$variant => $id),* }
            }

            pub fn label(self) -> &'static str {
                match self { $(Command::$variant => $label),* }
            }

            pub fn scope(self) -> Scope {
                match self { $(Command::$variant => $scope),* }
            }

            fn default_binding(self) -> Binding {
                match self { $(Command::$variant => Binding { mods: $mods, key: $key }),* }
            }

            pub fn from_id(id: &str) -> Option<Command> {
                match id { $($id => Some(Command::$variant),)* _ => None }
            }
        }
    };
}

commands![
    (Open, "open", "Open a file", Scope::Global, Modifiers::COMMAND, Key::O),
    (Reload, "reload", "Reload this file", Scope::Global, Modifiers::COMMAND, Key::R),
    (SaveProject, "save-project", "Save the project", Scope::Global, Modifiers::COMMAND, Key::S),
    (CloseTab, "close-tab", "Close this tab", Scope::Global, Modifiers::COMMAND, Key::W),
    (Goto, "goto", "Go to an address", Scope::Global, Modifiers::COMMAND, Key::G),
    (Palette, "palette", "Go to anything", Scope::Global, Modifiers::COMMAND, Key::P),
    (Back, "back", "Back", Scope::Global, Modifiers::ALT, Key::ArrowLeft),
    (Forward, "forward", "Forward", Scope::Global, Modifiers::ALT, Key::ArrowRight),
    (Dismiss, "dismiss", "Close what is open", Scope::Global, Modifiers::NONE, Key::Escape),
    (
        ToggleNavigator,
        "toggle-navigator",
        "Show or hide the navigator",
        Scope::Global,
        Modifiers::COMMAND,
        Key::Num1
    ),
    (
        ToggleInspector,
        "toggle-inspector",
        "Show or hide the inspector",
        Scope::Global,
        Modifiers::COMMAND,
        Key::Num2
    ),
    (SelectDown, "select-down", "Next line", Scope::Listing, Modifiers::NONE, Key::ArrowDown),
    (SelectUp, "select-up", "Previous line", Scope::Listing, Modifiers::NONE, Key::ArrowUp),
    (PageDown, "page-down", "Down a page", Scope::Listing, Modifiers::NONE, Key::PageDown),
    (PageUp, "page-up", "Up a page", Scope::Listing, Modifiers::NONE, Key::PageUp),
    (
        Follow,
        "follow",
        "Follow what this line points at",
        Scope::Listing,
        Modifiers::NONE,
        Key::Enter
    ),
    (Rename, "rename", "Name this function", Scope::Selection, Modifiers::NONE, Key::N),
    (Bookmark, "bookmark", "Bookmark this line", Scope::Selection, Modifiers::NONE, Key::B),
];

/// One key with its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub mods: Modifiers,
    pub key: Key,
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<&str> = Vec::new();
        // `COMMAND` is Cmd on macOS and Ctrl everywhere else, so it is
        // written as whatever the reader will actually press.
        if self.mods.command {
            parts.push(if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" });
        }
        if self.mods.ctrl && !self.mods.command {
            parts.push("Ctrl");
        }
        if self.mods.alt {
            parts.push(if cfg!(target_os = "macos") { "Option" } else { "Alt" });
        }
        if self.mods.shift {
            parts.push("Shift");
        }
        let name = key_name(self.key);
        parts.push(&name);
        write!(f, "{}", parts.join("+"))
    }
}

fn key_name(key: Key) -> String {
    match key {
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        Key::Enter => "Enter".into(),
        Key::Escape => "Esc".into(),
        Key::PageUp => "PgUp".into(),
        Key::PageDown => "PgDn".into(),
        other => {
            let s = format!("{other:?}");
            // egui names the digits `Num0`..`Num9`, which is not what is
            // printed on the key.
            s.strip_prefix("Num").map(str::to_owned).unwrap_or(s)
        }
    }
}

impl Binding {
    /// The canonical form of a set of modifiers.
    ///
    /// A key press reports the raw state -- on Linux a real Ctrl press sets
    /// both `ctrl` and `command` -- while the defaults are written with
    /// `Modifiers::COMMAND`, which sets only `command`. Comparing the two
    /// directly says they differ, so a captured Ctrl+O would not be
    /// recognised as the key Ctrl+O is already bound to.
    pub fn normalized(self) -> Binding {
        Binding {
            mods: Modifiers {
                alt: self.mods.alt,
                shift: self.mods.shift,
                ctrl: false,
                mac_cmd: false,
                command: self.mods.command || self.mods.ctrl,
            },
            key: self.key,
        }
    }

    /// Parse the form [`Display`] produces.
    pub fn parse(text: &str) -> Option<Binding> {
        let mut mods = Modifiers::NONE;
        let mut key = None;
        for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "command" | "control" => mods.command = true,
                "alt" | "option" => mods.alt = true,
                "shift" => mods.shift = true,
                _ => key = parse_key(part),
            }
        }
        Some(Binding { mods, key: key? }.normalized())
    }
}

fn parse_key(name: &str) -> Option<Key> {
    let n = name.to_ascii_lowercase();
    let named = match n.as_str() {
        "left" => Some(Key::ArrowLeft),
        "right" => Some(Key::ArrowRight),
        "up" => Some(Key::ArrowUp),
        "down" => Some(Key::ArrowDown),
        "enter" | "return" => Some(Key::Enter),
        "esc" | "escape" => Some(Key::Escape),
        "pgup" | "pageup" => Some(Key::PageUp),
        "pgdn" | "pagedown" => Some(Key::PageDown),
        "space" => Some(Key::Space),
        "tab" => Some(Key::Tab),
        _ => None,
    };
    named.or_else(|| Key::from_name(name)).or_else(|| {
        // A bare digit is stored as `Num3`.
        (name.len() == 1 && name.chars().all(|c| c.is_ascii_digit()))
            .then(|| Key::from_name(&format!("Num{name}")))
            .flatten()
    })
}

/// Whether a key is a modifier, and so cannot be a binding by itself.
///
/// The modifier arrives as its own key press before the one it modifies, so
/// a capture that takes the first press it sees ends up binding Ctrl+Ctrl.
/// The names are matched rather than listed because they vary by platform and
/// by side: `ControlLeft`, `MetaRight`, and so on.
pub fn is_modifier_key(key: Key) -> bool {
    let name = format!("{key:?}");
    ["Control", "Shift", "Alt", "Meta", "Super", "Command", "Option"]
        .iter()
        .any(|m| name.contains(m))
}

/// What every command is bound to.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: BTreeMap<Command, Binding>,
}

impl Default for Keymap {
    fn default() -> Keymap {
        Keymap {
            bindings: Command::ALL
                .iter()
                .map(|c| (*c, c.default_binding()))
                .collect(),
        }
    }
}

/// The on-disk form: command id to the key that runs it.
#[derive(Serialize, Deserialize, Default)]
struct Saved(BTreeMap<String, String>);

impl Keymap {
    pub fn binding(&self, c: Command) -> Binding {
        self.bindings
            .get(&c)
            .copied()
            .unwrap_or_else(|| c.default_binding())
    }

    /// The shortcut as it should appear beside a menu entry.
    pub fn shortcut(&self, c: Command) -> String {
        self.binding(c).to_string()
    }

    pub fn is_default(&self, c: Command) -> bool {
        self.binding(c) == c.default_binding()
    }

    pub fn reset(&mut self, c: Command) {
        self.bindings.insert(c, c.default_binding());
    }

    /// The other command already using this key, if any.
    ///
    /// Checked across every scope, not within one. It is tempting to allow a
    /// key to mean two things in two scopes, but all three scopes here are
    /// live at once in a listing with a line selected -- which is where the
    /// tool is nearly all the time -- so a shared key would simply be a
    /// shortcut that sometimes does the wrong thing.
    pub fn conflict(&self, c: Command, b: Binding) -> Option<Command> {
        let b = b.normalized();
        self.bindings
            .iter()
            .find(|(other, existing)| **other != c && existing.normalized() == b)
            .map(|(other, _)| *other)
    }

    pub fn bind(&mut self, c: Command, b: Binding) {
        self.bindings.insert(c, b.normalized());
    }

    pub fn load() -> Keymap {
        let mut map = Keymap::default();
        let Some(path) = crate::paths::keys_file() else {
            return map;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return map;
        };
        let Ok(saved) = serde_json::from_str::<Saved>(&text) else {
            return map;
        };
        for (id, key) in saved.0 {
            // An unknown command id is a binding for a command this build
            // does not have. Dropping it silently is right: the file may come
            // from a newer version, and it will still be there next time.
            if let (Some(c), Some(b)) = (Command::from_id(&id), Binding::parse(&key)) {
                map.bindings.insert(c, b);
            }
        }
        map
    }

    /// Write out only what differs from the defaults.
    ///
    /// A file full of defaults would freeze them: change a default later and
    /// every existing installation would keep the old one forever.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = crate::paths::keys_file() else {
            return Ok(());
        };
        crate::paths::ensure_parent(&path)?;
        let saved = Saved(
            self.bindings
                .iter()
                .filter(|(c, b)| **b != c.default_binding())
                .map(|(c, b)| (c.id().to_owned(), b.to_string()))
                .collect(),
        );
        let text = serde_json::to_string_pretty(&saved)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binding_survives_a_round_trip() {
        for c in Command::ALL {
            let b = c.default_binding();
            let text = b.to_string();
            assert_eq!(Binding::parse(&text), Some(b), "{} -> {text}", c.id());
        }
    }

    #[test]
    fn the_defaults_do_not_collide() {
        let map = Keymap::default();
        for c in Command::ALL {
            assert_eq!(
                map.conflict(*c, map.binding(*c)),
                None,
                "{} collides",
                c.id()
            );
        }
    }

    #[test]
    fn every_command_has_its_own_id() {
        let mut ids: Vec<&str> = Command::ALL.iter().map(|c| c.id()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn an_id_maps_back_to_its_command() {
        for c in Command::ALL {
            assert_eq!(Command::from_id(c.id()), Some(*c));
        }
        assert_eq!(Command::from_id("no-such-command"), None);
    }

    #[test]
    fn a_key_two_commands_want_is_a_conflict_across_scopes() {
        // A listing binding and a selection binding are both live in a
        // listing with a line selected, so sharing a key is not a way to
        // have it mean two things.
        let mut map = Keymap::default();
        map.bind(
            Command::SelectDown,
            Binding {
                mods: Modifiers::NONE,
                key: Key::N,
            },
        );
        assert_eq!(
            map.conflict(
                Command::Rename,
                Binding {
                    mods: Modifiers::NONE,
                    key: Key::N
                }
            ),
            Some(Command::SelectDown)
        );
    }

    #[test]
    fn only_changed_bindings_are_written() {
        let mut map = Keymap::default();
        assert!(map.is_default(Command::Open));
        map.bind(
            Command::Open,
            Binding {
                mods: Modifiers::COMMAND,
                key: Key::K,
            },
        );
        assert!(!map.is_default(Command::Open));
        map.reset(Command::Open);
        assert!(map.is_default(Command::Open));
    }

    #[test]
    fn a_modifier_is_not_a_binding() {
        for name in [
            "ControlLeft",
            "ControlRight",
            "ShiftLeft",
            "AltLeft",
            "MetaLeft",
            "SuperRight",
        ] {
            if let Some(k) = Key::from_name(name) {
                assert!(is_modifier_key(k), "{name} should not be bindable");
            }
        }
        for name in ["O", "Enter", "F5", "Num1", "ArrowLeft"] {
            if let Some(k) = Key::from_name(name) {
                assert!(!is_modifier_key(k), "{name} should be bindable");
            }
        }
    }

    #[test]
    fn a_raw_ctrl_press_is_the_same_key_as_the_default() {
        // What a key press reports on Linux, against how the defaults are
        // written. Before these are put in the same form, binding Ctrl+O
        // onto a command silently steals it from the one that has it.
        let pressed = Binding {
            mods: Modifiers {
                alt: false,
                ctrl: true,
                shift: false,
                mac_cmd: false,
                command: true,
            },
            key: Key::O,
        };
        let declared = Binding {
            mods: Modifiers::COMMAND,
            key: Key::O,
        };
        assert_ne!(pressed, declared);
        assert_eq!(pressed.normalized(), declared.normalized());

        let map = Keymap::default();
        assert_eq!(map.conflict(Command::Palette, pressed), Some(Command::Open));
    }

    #[test]
    fn digits_read_the_way_they_are_printed() {
        let b = Binding {
            mods: Modifiers::COMMAND,
            key: Key::Num1,
        };
        assert!(b.to_string().ends_with("+1"));
        assert_eq!(Binding::parse(&b.to_string()), Some(b));
    }
}
