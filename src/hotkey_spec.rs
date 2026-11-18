/// A global hotkey. At least one of Ctrl, Alt, or Win is required so the
/// shortcut does not swallow ordinary typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeySpec {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: VirtualKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualKey {
    Letter(u8),
    Digit(u8),
    Function(u8),
    Space,
    Tab,
    Enter,
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureFail {
    /// The event was only a modifier. Keep recording.
    Ignore,
    Cancelled,
    NeedsModifier,
    Unsupported,
}

impl HotkeySpec {
    pub fn parse(input: &str) -> Option<Self> {
        let tokens: Vec<&str> = input
            .split('+')
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .collect();
        if tokens.is_empty() {
            return None;
        }
        let mut spec = Self {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            key: VirtualKey::Letter(b'A'),
        };
        let mut key = None;
        for token in tokens {
            if let Some(parsed) = parse_modifier(token) {
                match parsed {
                    Modifier::Ctrl => spec.ctrl = true,
                    Modifier::Alt => spec.alt = true,
                    Modifier::Shift => spec.shift = true,
                    Modifier::Win => spec.win = true,
                }
            } else if key.is_some() {
                return None;
            } else {
                key = parse_key(token);
                if key.is_none() {
                    return None;
                }
            }
        }
        spec.key = key?;
        spec.is_valid().then_some(spec)
    }

    pub fn capture(
        ctrl: bool,
        alt: bool,
        shift: bool,
        win: bool,
        key_text: &str,
    ) -> Result<Self, CaptureFail> {
        let text = key_text.trim();
        if text.eq_ignore_ascii_case("escape") {
            return Err(CaptureFail::Cancelled);
        }
        if is_modifier_text(text) {
            return Err(CaptureFail::Ignore);
        }
        let key = parse_key(text).ok_or(CaptureFail::Unsupported)?;
        let spec = Self {
            ctrl,
            alt,
            shift,
            win,
            key,
        };
        if spec.is_valid() {
            Ok(spec)
        } else {
            Err(CaptureFail::NeedsModifier)
        }
    }

    pub fn is_valid(self) -> bool {
        self.ctrl || self.alt || self.win
    }

    /// `ctrl+alt+t`
    pub fn serialize(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl".to_string());
        }
        if self.alt {
            parts.push("alt".to_string());
        }
        if self.shift {
            parts.push("shift".to_string());
        }
        if self.win {
            parts.push("win".to_string());
        }
        parts.push(self.key.token());
        parts.join("+")
    }

    /// Tray menu, matching the mockup: `Ctrl+Alt+T`.
    pub fn display_compact(self) -> String {
        self.display_parts().join("+")
    }

    /// Settings field, matching the mockup: `Ctrl + Alt + T`.
    pub fn display_spaced(self) -> String {
        self.display_parts().join(" + ")
    }

    fn display_parts(self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.win {
            parts.push("Win".to_string());
        }
        parts.push(self.key.label());
        parts
    }

    /// Win32 `RegisterHotKey` modifiers, including `MOD_NOREPEAT`.
    pub fn modifier_bits(self) -> u32 {
        let mut bits = 0x4000;
        if self.alt {
            bits |= 0x0001;
        }
        if self.ctrl {
            bits |= 0x0002;
        }
        if self.shift {
            bits |= 0x0004;
        }
        if self.win {
            bits |= 0x0008;
        }
        bits
    }
}

impl VirtualKey {
    pub fn virtual_key(self) -> u32 {
        match self {
            Self::Letter(ch) | Self::Digit(ch) => u32::from(ch),
            Self::Function(n) => 0x70 + u32::from(n - 1),
            Self::Space => 0x20,
            Self::Tab => 0x09,
            Self::Enter => 0x0D,
            Self::Left => 0x25,
            Self::Up => 0x26,
            Self::Right => 0x27,
            Self::Down => 0x28,
        }
    }

    fn token(self) -> String {
        match self {
            Self::Letter(ch) => char::from(ch).to_ascii_lowercase().to_string(),
            Self::Digit(ch) => char::from(ch).to_string(),
            Self::Function(n) => format!("f{n}"),
            Self::Space => "space".to_string(),
            Self::Tab => "tab".to_string(),
            Self::Enter => "enter".to_string(),
            Self::Left => "left".to_string(),
            Self::Right => "right".to_string(),
            Self::Up => "up".to_string(),
            Self::Down => "down".to_string(),
        }
    }

    fn label(self) -> String {
        match self {
            Self::Letter(ch) | Self::Digit(ch) => char::from(ch).to_string(),
            Self::Function(n) => format!("F{n}"),
            Self::Space => "Space".to_string(),
            Self::Tab => "Tab".to_string(),
            Self::Enter => "Enter".to_string(),
            Self::Left => "Left".to_string(),
            Self::Right => "Right".to_string(),
            Self::Up => "Up".to_string(),
            Self::Down => "Down".to_string(),
        }
    }
}

enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

fn parse_modifier(token: &str) -> Option<Modifier> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifier::Ctrl),
        "alt" => Some(Modifier::Alt),
        "shift" => Some(Modifier::Shift),
        "win" | "meta" | "super" => Some(Modifier::Win),
        _ => None,
    }
}

fn is_modifier_text(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "control" | "ctrl" | "alt" | "shift" | "meta" | "win" | "super"
    )
}

fn parse_key(token: &str) -> Option<VirtualKey> {
    let lower = token.to_ascii_lowercase();
    let mut chars = lower.chars();
    let first = chars.next()?;
    let rest_empty = chars.next().is_none();
    if rest_empty && first.is_ascii_alphabetic() {
        return Some(VirtualKey::Letter(first.to_ascii_uppercase() as u8));
    }
    if rest_empty && first.is_ascii_digit() {
        return Some(VirtualKey::Digit(first as u8));
    }
    if let Some(number) = lower.strip_prefix('f') {
        if let Ok(n) = number.parse::<u8>() {
            if (1..=12).contains(&n) {
                return Some(VirtualKey::Function(n));
            }
        }
    }
    Some(match lower.as_str() {
        "space" => VirtualKey::Space,
        "tab" => VirtualKey::Tab,
        "enter" | "return" => VirtualKey::Enter,
        "left" => VirtualKey::Left,
        "right" => VirtualKey::Right,
        "up" => VirtualKey::Up,
        "down" => VirtualKey::Down,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkey_matches_the_mockup() {
        let spec = HotkeySpec::parse("ctrl+alt+t").unwrap();
        assert!(spec.ctrl && spec.alt && !spec.shift && !spec.win);
        assert_eq!(spec.key, VirtualKey::Letter(b'T'));
        assert_eq!(spec.display_compact(), "Ctrl+Alt+T");
        assert_eq!(spec.display_spaced(), "Ctrl + Alt + T");
        assert_eq!(spec.serialize(), "ctrl+alt+t");
        assert_eq!(spec.modifier_bits(), 0x4003);
        assert_eq!(spec.key.virtual_key(), 0x54);
        assert_eq!(HotkeySpec::parse("Ctrl + Alt + T"), Some(spec));
    }

    #[test]
    fn roundtrip_and_rejects_shift_only() {
        let spec = HotkeySpec::parse("ctrl+shift+f5").unwrap();
        assert_eq!(HotkeySpec::parse(&spec.serialize()), Some(spec));
        assert_eq!(spec.display_spaced(), "Ctrl + Shift + F5");
        assert_eq!(spec.key.virtual_key(), 0x74);
        assert!(HotkeySpec::parse("shift+a").is_none());
        assert!(HotkeySpec::parse("t").is_none());
        assert!(HotkeySpec::parse("").is_none());
    }

    #[test]
    fn capture_from_a_slint_key_event() {
        let spec = HotkeySpec::capture(true, true, false, false, "t").unwrap();
        assert_eq!(spec.serialize(), "ctrl+alt+t");
        assert_eq!(
            HotkeySpec::capture(false, false, true, false, "A"),
            Err(CaptureFail::NeedsModifier)
        );
        assert_eq!(
            HotkeySpec::capture(true, false, false, false, "Control"),
            Err(CaptureFail::Ignore)
        );
        assert_eq!(
            HotkeySpec::capture(true, false, false, false, "Escape"),
            Err(CaptureFail::Cancelled)
        );
        assert_eq!(
            HotkeySpec::capture(true, false, false, false, "Insert"),
            Err(CaptureFail::Unsupported)
        );
    }
}
