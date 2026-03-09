use crate::makcu::error::{MakcuError, MakcuResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Letter(char),
    Number(char),
    Function(u8),
    System(SystemKey),
    Modifier(ModifierKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemKey {
    Enter,
    Escape,
    Backspace,
    Tab,
    Space,
    PrintScreen,
    ScrollLock,
    Pause,
    Insert,
    Home,
    PageUp,
    Delete,
    End,
    PageDown,
    Right,
    Left,
    Down,
    Up,
    NumLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKey {
    LeftCtrl,
    RightCtrl,
    LeftShift,
    RightShift,
    LeftAlt,
    RightAlt,
    LeftGui,
    RightGui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardControl;

impl KeyboardControl {
    pub fn build_down_command(key: Key) -> String {
        format!(".down({})\r\n", Self::key_to_string(key))
    }

    pub fn build_up_command(key: Key) -> String {
        format!(".up({})\r\n", Self::key_to_string(key))
    }

    pub fn build_press_command(key: Key, hold_ms: Option<u16>, rand_ms: Option<u8>) -> MakcuResult<String> {
        let mut cmd = format!(".press({}", Self::key_to_string(key));

        if let Some(hold) = hold_ms {
            cmd.push_str(&format!(",{}", hold));
        }

        if let Some(rand) = rand_ms {
            cmd.push_str(&format!(",{}", rand));
        }

        cmd.push_str(")\r\n");
        Ok(cmd)
    }

    pub fn build_string_command(text: &str) -> MakcuResult<String> {
        if text.len() > 256 {
            return Err(MakcuError::InvalidParameter(
                "字符串长度不能超过256个字符".to_string(),
            ));
        }

        Ok(format!(".string({})\r\n", text))
    }

    pub fn build_init_command() -> String {
        ".init()\r\n".to_string()
    }

    pub fn build_isdown_command(key: Key) -> String {
        format!(".isdown({})\r\n", Self::key_to_string(key))
    }

    pub fn build_disable_command(keys: Vec<Key>) -> String {
        if keys.is_empty() {
            return ".disable()\r\n".to_string();
        }

        let key_strs: Vec<String> = keys
            .iter()
            .map(|k| Self::key_to_string(*k))
            .collect();

        format!(".disable({})\r\n", key_strs.join(","))
    }

    pub fn build_enable_command(key: Key) -> String {
        format!(".disable({},0)\r\n", Self::key_to_string(key))
    }

    pub fn build_mask_command(key: Key, mode: u8) -> String {
        format!(".mask({},{})\r\n", Self::key_to_string(key), mode)
    }

    pub fn build_remap_command(source: Key, target: Key) -> String {
        format!(
            ".remap({},{})\r\n",
            Self::key_to_string(source),
            Self::key_to_string(target)
        )
    }

    pub fn build_clear_remap_command(key: Key) -> String {
        format!(".remap({},0)\r\n", Self::key_to_string(key))
    }

    pub fn build_reset_remap_command() -> String {
        ".remap(0)\r\n".to_string()
    }

    fn key_to_string(key: Key) -> String {
        match key {
            Key::Letter(c) => c.to_string(),
            Key::Number(c) => c.to_string(),
            Key::Function(n) => format!("f{}", n),
            Key::System(s) => Self::system_key_to_string(s),
            Key::Modifier(m) => Self::modifier_key_to_string(m),
        }
    }

    fn system_key_to_string(key: SystemKey) -> String {
        match key {
            SystemKey::Enter => "enter".to_string(),
            SystemKey::Escape => "escape".to_string(),
            SystemKey::Backspace => "backspace".to_string(),
            SystemKey::Tab => "tab".to_string(),
            SystemKey::Space => "space".to_string(),
            SystemKey::PrintScreen => "printscreen".to_string(),
            SystemKey::ScrollLock => "scrolllock".to_string(),
            SystemKey::Pause => "pause".to_string(),
            SystemKey::Insert => "insert".to_string(),
            SystemKey::Home => "home".to_string(),
            SystemKey::PageUp => "pageup".to_string(),
            SystemKey::Delete => "delete".to_string(),
            SystemKey::End => "end".to_string(),
            SystemKey::PageDown => "pagedown".to_string(),
            SystemKey::Right => "right".to_string(),
            SystemKey::Left => "left".to_string(),
            SystemKey::Down => "down".to_string(),
            SystemKey::Up => "up".to_string(),
            SystemKey::NumLock => "numlock".to_string(),
        }
    }

    fn modifier_key_to_string(key: ModifierKey) -> String {
        match key {
            ModifierKey::LeftCtrl | ModifierKey::RightCtrl => "ctrl".to_string(),
            ModifierKey::LeftShift | ModifierKey::RightShift => "shift".to_string(),
            ModifierKey::LeftAlt | ModifierKey::RightAlt => "alt".to_string(),
            ModifierKey::LeftGui | ModifierKey::RightGui => "win".to_string(),
        }
    }
}
