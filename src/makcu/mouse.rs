use crate::makcu::error::{MakcuError, MakcuResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtons {
    Left = 1,
    Right = 2,
    Middle = 3,
    Side1 = 4,
    Side2 = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAxis {
    X,
    Y,
    Wheel,
    Pan,
    Tilt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockState {
    Unlocked = 0,
    Locked = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockDirection {
    Both,
    Positive,
    Negative,
}

pub struct MouseControl;

impl MouseControl {
    pub fn build_get_button_command(button: MouseButtons) -> String {
        format!(".{}()\r\n", Self::button_name(button))
    }

    pub fn build_set_button_command(button: MouseButtons, state: u8) -> String {
        format!(".{}({})\r\n", Self::button_name(button), state)
    }

    pub fn build_click_command(button: MouseButtons, count: u8) -> String {
        format!(".click({},{})\r\n", button as u8, count)
    }

    pub fn build_click_with_delay_command(
        button: MouseButtons,
        count: u8,
        delay_ms: u16,
    ) -> MakcuResult<String> {
        if delay_ms > 5000 {
            return Err(MakcuError::InvalidParameter(
                "延迟不能超过5000ms".to_string(),
            ));
        }
        Ok(format!(
            ".click({},{},{})\r\n",
            button as u8,
            count,
            delay_ms
        ))
    }

    pub fn build_turbo_command(
        button: MouseButtons,
        delay_ms: u16,
    ) -> MakcuResult<String> {
        if delay_ms > 5000 {
            return Err(MakcuError::InvalidParameter(
                "延迟不能超过5000ms".to_string(),
            ));
        }
        Ok(format!(".turbo({},{})\r\n", button as u8, delay_ms))
    }

    pub fn build_disable_turbo_command(button: MouseButtons) -> String {
        format!(".turbo({},0)\r\n", button as u8)
    }

    pub fn build_disable_all_turbo_command() -> String {
        ".turbo(0)\r\n".to_string()
    }

    pub fn build_move_command(
        dx: i16,
        dy: i16,
        segments: Option<u16>,
        control_points: Option<[(i16, i16); 2]>,
    ) -> MakcuResult<String> {
        let segments = segments.unwrap_or(1);
        if segments > 512 {
            return Err(MakcuError::InvalidParameter(
                "分段数不能超过512".to_string(),
            ));
        }

        let mut cmd = format!(".move({},{},{}", dx, dy, segments);

        if let Some(points) = control_points {
            cmd.push_str(&format!(
                ",{},{},{},{}",
                points[0].0, points[0].1, points[1].0, points[1].1
            ));
        }

        cmd.push_str(")\r\n");
        Ok(cmd)
    }

    pub fn build_moveto_command(
        x: u16,
        y: u16,
        segments: Option<u16>,
        control_points: Option<[(i16, i16); 2]>,
    ) -> MakcuResult<String> {
        let segments = segments.unwrap_or(1);
        if segments > 512 {
            return Err(MakcuError::InvalidParameter(
                "分段数不能超过512".to_string(),
            ));
        }

        let mut cmd = format!(".moveto({},{},{}", x, y, segments);

        if let Some(points) = control_points {
            cmd.push_str(&format!(
                ",{},{},{},{}",
                points[0].0, points[0].1, points[1].0, points[1].1
            ));
        }

        cmd.push_str(")\r\n");
        Ok(cmd)
    }

    pub fn build_wheel_command(delta: i8) -> String {
        let clamped = if delta > 0 { 1 } else if delta < 0 { -1 } else { 0 };
        format!(".wheel({})\r\n", clamped)
    }

    pub fn build_pan_command(steps: i16) -> String {
        format!(".pan({})\r\n", steps)
    }

    pub fn build_tilt_command(steps: i16) -> String {
        format!(".tilt({})\r\n", steps)
    }

    pub fn build_getpos_command() -> String {
        ".getpos()\r\n".to_string()
    }

    pub fn build_silent_command(x: u16, y: u16) -> String {
        format!(".silent({},{})\r\n", x, y)
    }

    pub fn build_lock_axis_command(
        axis: MouseAxis,
        direction: LockDirection,
        state: LockState,
    ) -> String {
        let axis_name = match axis {
            MouseAxis::X => "mx",
            MouseAxis::Y => "my",
            MouseAxis::Wheel => "mw",
            MouseAxis::Pan | MouseAxis::Tilt => return String::new(),
        };

        let direction_suffix = match direction {
            LockDirection::Both => "",
            LockDirection::Positive => "+",
            LockDirection::Negative => "-",
        };

        format!(
            ".lock_{}({})\r\n",
            format!("{}{}", axis_name, direction_suffix),
            state as u8
        )
    }

    pub fn build_lock_button_command(
        button: MouseButtons,
        state: LockState,
    ) -> String {
        let button_name = match button {
            MouseButtons::Left => "ml",
            MouseButtons::Middle => "mm",
            MouseButtons::Right => "mr",
            MouseButtons::Side1 => "ms1",
            MouseButtons::Side2 => "ms2",
        };
        format!(".lock_{}({})\r\n", button_name, state as u8)
    }

    pub fn build_catch_command(
        button: MouseButtons,
        mode: u8,
    ) -> String {
        let button_name = match button {
            MouseButtons::Left => "ml",
            MouseButtons::Middle => "mm",
            MouseButtons::Right => "mr",
            MouseButtons::Side1 => "ms1",
            MouseButtons::Side2 => "ms2",
        };
        format!(".catch_{}({})\r\n", button_name, mode)
    }

    pub fn build_remap_button_command(
        src: MouseButtons,
        dst: MouseButtons,
    ) -> String {
        format!(
            ".remap_button({},{})\r\n",
            src as u8,
            dst as u8
        )
    }

    pub fn build_reset_button_remap_command() -> String {
        ".remap_button(0)\r\n".to_string()
    }

    pub fn build_remap_axis_command(
        invert_x: bool,
        invert_y: bool,
        swap_xy: bool,
    ) -> String {
        format!(
            ".remap_axis({},{},{})\r\n",
            if invert_x { 1 } else { 0 },
            if invert_y { 1 } else { 0 },
            if swap_xy { 1 } else { 0 }
        )
    }

    pub fn build_reset_axis_remap_command() -> String {
        ".remap_axis(0)\r\n".to_string()
    }

    fn button_name(button: MouseButtons) -> &'static str {
        match button {
            MouseButtons::Left => "left",
            MouseButtons::Right => "right",
            MouseButtons::Middle => "middle",
            MouseButtons::Side1 => "side1",
            MouseButtons::Side2 => "side2",
        }
    }
}
