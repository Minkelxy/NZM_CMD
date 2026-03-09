use crate::makcu::error::{MakcuError, MakcuResult};

const FRAME_HEAD: u8 = 0xDE;
const FRAME_TAIL: u8 = 0xAD;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedTarget {
    Device = 1,
    Host = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedMode {
    Off = 0,
    On = 1,
    SlowBlink = 2,
    FastBlink = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedState {
    pub target: LedTarget,
    pub mode: LedMode,
}

pub struct LedControl;

impl LedControl {
    pub fn build_query_command(target: LedTarget) -> String {
        format!(".led({})\r\n", target as u8)
    }

    pub fn build_set_command(target: LedTarget, mode: LedMode) -> String {
        format!(".led({},{})\r\n", target as u8, mode as u8)
    }

    pub fn build_blink_command(
        target: LedTarget,
        times: u8,
        delay_ms: u16,
    ) -> MakcuResult<String> {
        if times == 0 {
            return Err(MakcuError::InvalidParameter(
                "闪烁次数必须大于0".to_string(),
            ));
        }
        if delay_ms > 5000 {
            return Err(MakcuError::InvalidParameter(
                "延迟不能超过5000ms".to_string(),
            ));
        }

        Ok(format!(
            ".led({},{},{})\r\n",
            target as u8,
            times,
            delay_ms
        ))
    }

    pub fn parse_response(response: &str) -> Option<LedState> {
        let response = response.trim().trim_start_matches("km.");
        if !response.starts_with("led(") {
            return None;
        }

        let content = response.strip_prefix("led(")?.strip_suffix(")")?;
        let parts: Vec<&str> = content.split(',').collect();

        if parts.len() != 2 {
            return None;
        }

        let target = match parts[0].trim() {
            "device" | "1" => LedTarget::Device,
            "host" | "2" => LedTarget::Host,
            _ => return None,
        };

        let mode = match parts[1].trim() {
            "off" | "0" => LedMode::Off,
            "on" | "1" => LedMode::On,
            "slow_blink" | "2" => LedMode::SlowBlink,
            "fast_blink" | "3" => LedMode::FastBlink,
            _ => return None,
        };

        Some(LedState { target, mode })
    }
}
