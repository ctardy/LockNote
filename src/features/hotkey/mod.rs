// Global Hotkey module
//
// Registers a system-wide hotkey using the Windows RegisterHotKey API.
// Default: Ctrl+Shift+L — toggles window minimize/restore.

/// Hotkey modifier flags (matches Windows MOD_ constants).
pub mod modifiers {
    pub const ALT: u32 = 1;
    pub const CTRL: u32 = 2;
    pub const SHIFT: u32 = 4;
}

/// Hotkey ID used for RegisterHotKey ("LN" = 0x4C4E).
pub const HOTKEY_ID: i32 = 0x4C4E;

/// WM_HOTKEY message constant.
pub const WM_HOTKEY: u32 = 0x0312;

/// Global hotkey configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalHotkey {
    /// Modifier bitmask (Alt=1, Ctrl=2, Shift=4).
    pub modifiers: u32,
    /// Virtual key code.
    pub key: u32,
}

/// Default: Ctrl+Shift+L (modifiers=6, key=76).
impl Default for GlobalHotkey {
    fn default() -> Self {
        Self {
            modifiers: modifiers::CTRL | modifiers::SHIFT, // 6
            key: 76, // 'L'
        }
    }
}

impl GlobalHotkey {
    /// Create from settings values.
    pub fn from_settings(mods: u32, key: u32) -> Self {
        Self {
            modifiers: mods,
            key,
        }
    }

    /// Check if at least one modifier is set (required by Windows).
    pub fn is_valid(&self) -> bool {
        self.modifiers != 0 && self.key != 0
    }

    /// Register the hotkey with Windows.
    #[cfg(windows)]
    pub fn register(&self, hwnd: isize) -> bool {
        if !self.is_valid() {
            return false;
        }
        unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey(
                Some(windows::Win32::Foundation::HWND(hwnd as *mut _)),
                HOTKEY_ID,
                windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(self.modifiers),
                self.key,
            )
            .is_ok()
        }
    }

    /// Unregister the hotkey.
    #[cfg(windows)]
    pub fn unregister(&self, hwnd: isize) -> bool {
        unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey(
                Some(windows::Win32::Foundation::HWND(hwnd as *mut _)),
                HOTKEY_ID,
            )
            .is_ok()
        }
    }

    /// Human-readable display of the hotkey combo.
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers & modifiers::CTRL != 0 {
            parts.push("Ctrl");
        }
        if self.modifiers & modifiers::ALT != 0 {
            parts.push("Alt");
        }
        if self.modifiers & modifiers::SHIFT != 0 {
            parts.push("Shift");
        }
        // Convert virtual key code to character
        let key_name = match self.key {
            65..=90 => {
                let c = (self.key as u8) as char;
                c.to_string()
            }
            112..=123 => format!("F{}", self.key - 111), // F1-F12
            _ => format!("0x{:02X}", self.key),
        };
        parts.push(&key_name);
        // We need to own all strings for joining
        let owned: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
        owned.join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_ctrl_shift_l() {
        let hk = GlobalHotkey::default();
        assert_eq!(hk.modifiers, 6);
        assert_eq!(hk.key, 76);
        assert!(hk.is_valid());
    }

    #[test]
    fn display_default() {
        let hk = GlobalHotkey::default();
        assert_eq!(hk.display(), "Ctrl+Shift+L");
    }

    #[test]
    fn no_modifiers_is_invalid() {
        let hk = GlobalHotkey::from_settings(0, 76);
        assert!(!hk.is_valid());
    }

    #[test]
    fn no_key_is_invalid() {
        let hk = GlobalHotkey::from_settings(6, 0);
        assert!(!hk.is_valid());
    }

    #[test]
    fn from_settings() {
        let hk = GlobalHotkey::from_settings(3, 65); // Ctrl+Alt+A
        assert!(hk.is_valid());
        assert_eq!(hk.display(), "Ctrl+Alt+A");
    }
}
