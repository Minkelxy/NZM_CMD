use serialport::SerialPort;
use std::io::Write;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};

use crate::makcu::{
    config::MakcuConfig,
    error::{MakcuError, MakcuResult},
    mouse::{MouseButtons, MouseControl},
    keyboard::{Key, KeyboardControl},
    led::{LedTarget, LedMode, LedControl},
};

pub struct MakcuClient {
    port: Box<dyn SerialPort>,
    config: MakcuConfig,
    response_buffer: Arc<Mutex<String>>,
}

impl MakcuClient {
    pub fn new(config: MakcuConfig) -> MakcuResult<Self> {
        let port = serialport::new(&config.port_name, config.baud_rate)
            .timeout(config.timeout_duration())
            .open()
            .map_err(|e| MakcuError::SerialPortError(format!(
                "无法打开串口 {}: {}",
                config.port_name, e
            )))?;

        Ok(Self {
            port,
            config,
            response_buffer: Arc::new(Mutex::new(String::new())),
        })
    }

    pub fn send_command(&mut self, command: &str) -> MakcuResult<String> {
        self.port
            .write_all(command.as_bytes())
            .map_err(|e| MakcuError::CommandFailed(format!("发送命令失败: {}", e)))?;

        self.port.flush().map_err(|e| MakcuError::CommandFailed(format!("刷新失败: {}", e)))?;

        thread::sleep(Duration::from_millis(10));

        self.read_response()
    }

    fn read_response(&self) -> MakcuResult<String> {
        let mut buffer = String::new();
        let start = std::time::Instant::now();

        while start.elapsed() < self.config.timeout_duration() {
            let mut byte = [0u8; 1];
            match self.port.read(&mut byte) {
                Ok(_) => {
                    let ch = byte[0] as char;
                    buffer.push(ch);

                    if buffer.ends_with(">>>\r\n") || buffer.ends_with(">>>\n") {
                        let response = buffer.trim_end_matches(">>>\r\n").trim_end_matches(">>>\n").to_string();
                        *self.response_buffer.lock().unwrap() = response.clone();
                        return Ok(response);
                    }
                }
                Err(_) => {
                    if !buffer.is_empty() {
                        break;
                    }
                }
            }
        }

        Ok(buffer)
    }

    pub fn send_command_no_wait(&mut self, command: &str) -> MakcuResult<()> {
        self.port
            .write_all(command.as_bytes())
            .map_err(|e| MakcuError::CommandFailed(format!("发送命令失败: {}", e)))?;

        self.port.flush().map_err(|e| MakcuError::CommandFailed(format!("刷新失败: {}", e)))?;
        Ok(())
    }

    pub fn get_last_response(&self) -> String {
        self.response_buffer.lock().unwrap().clone()
    }

    pub fn clear_buffer(&mut self) {
        *self.response_buffer.lock().unwrap() = String::new();
    }

    pub fn send_binary_frame(&mut self, frame: &[u8]) -> MakcuResult<()> {
        self.port
            .write_all(frame)
            .map_err(|e| MakcuError::CommandFailed(format!("发送二进制帧失败: {}", e)))?;

        self.port.flush().map_err(|e| MakcuError::CommandFailed(format!("刷新失败: {}", e)))?;
        thread::sleep(Duration::from_millis(4));
        Ok(())
    }

    pub fn help(&mut self) -> MakcuResult<String> {
        self.send_command(".help()\r\n")
    }

    pub fn info(&mut self) -> MakcuResult<String> {
        self.send_command(".info()\r\n")
    }

    pub fn version(&mut self) -> MakcuResult<String> {
        self.send_command(".version()\r\n")
    }

    pub fn device(&mut self) -> MakcuResult<String> {
        self.send_command(".device()\r\n")
    }

    pub fn reboot(&mut self) -> MakcuResult<()> {
        self.send_command_no_wait(".reboot()\r\n")
    }

    pub fn serial(&mut self, text: Option<&str>) -> MakcuResult<String> {
        let cmd = match text {
            Some(t) => format!(".serial({})\r\n", t),
            None => ".serial()\r\n".to_string(),
        };
        self.send_command(&cmd)
    }

    pub fn log(&mut self, level: Option<u8>) -> MakcuResult<String> {
        let cmd = match level {
            Some(l) => format!(".log({})\r\n", l),
            None => ".log()\r\n".to_string(),
        };
        self.send_command(&cmd)
    }

    pub fn echo(&mut self, enable: Option<bool>) -> MakcuResult<String> {
        let cmd = match enable {
            Some(e) => format!(".echo({})\r\n", if e { 1 } else { 0 }),
            None => ".echo()\r\n".to_string(),
        };
        self.send_command(&cmd)
    }

    pub fn baud(&mut self, rate: Option<u32>) -> MakcuResult<String> {
        let cmd = match rate {
            Some(r) => format!(".baud({})\r\n", r),
            None => ".baud()\r\n".to_string(),
        };
        self.send_command(&cmd)
    }

    pub fn bypass(&mut self, mode: Option<u8>) -> MakcuResult<String> {
        let cmd = match mode {
            Some(m) => format!(".bypass({})\r\n", m),
            None => ".bypass()\r\n".to_string(),
        };
        self.send_command(&cmd)
    }

    pub fn hs(&mut self, enable: Option<bool>) -> MakcuResult<String> {
        let cmd = match enable {
            Some(e) => format!(".hs({})\r\n", if e { 1 } else { 0 }),
            None => ".hs()\r\n".to_string(),
        };
        self.send_command(&cmd)
    }

    pub fn release(&mut self, timer_ms: Option<u16>) -> MakcuResult<()> {
        let cmd = match timer_ms {
            Some(t) => format!(".release({})\r\n", t),
            None => ".release()\r\n".to_string(),
        };
        self.send_command_no_wait(&cmd)
    }

    pub fn fault(&mut self) -> MakcuResult<String> {
        self.send_command(".fault()\r\n")
    }

    pub fn mouse_left(&mut self, state: Option<u8>) -> MakcuResult<String> {
        let cmd = match state {
            Some(s) => MouseControl::build_set_button_command(MouseButtons::Left, s),
            None => MouseControl::build_get_button_command(MouseButtons::Left),
        };
        self.send_command(&cmd)
    }

    pub fn mouse_right(&mut self, state: Option<u8>) -> MakcuResult<String> {
        let cmd = match state {
            Some(s) => MouseControl::build_set_button_command(MouseButtons::Right, s),
            None => MouseControl::build_get_button_command(MouseButtons::Right),
        };
        self.send_command(&cmd)
    }

    pub fn mouse_middle(&mut self, state: Option<u8>) -> MakcuResult<String> {
        let cmd = match state {
            Some(s) => MouseControl::build_set_button_command(MouseButtons::Middle, s),
            None => MouseControl::build_get_button_command(MouseButtons::Middle),
        };
        self.send_command(&cmd)
    }

    pub fn mouse_side1(&mut self, state: Option<u8>) -> MakcuResult<String> {
        let cmd = match state {
            Some(s) => MouseControl::build_set_button_command(MouseButtons::Side1, s),
            None => MouseControl::build_get_button_command(MouseButtons::Side1),
        };
        self.send_command(&cmd)
    }

    pub fn mouse_side2(&mut self, state: Option<u8>) -> MakcuResult<String> {
        let cmd = match state {
            Some(s) => MouseControl::build_set_button_command(MouseButtons::Side2, s),
            None => MouseControl::build_get_button_command(MouseButtons::Side2),
        };
        self.send_command(&cmd)
    }

    pub fn mouse_click(&mut self, button: MouseButtons, count: u8) -> MakcuResult<String> {
        let cmd = MouseControl::build_click_command(button, count);
        self.send_command(&cmd)
    }

    pub fn mouse_click_with_delay(
        &mut self,
        button: MouseButtons,
        count: u8,
        delay_ms: u16,
    ) -> MakcuResult<String> {
        let cmd = MouseControl::build_click_with_delay_command(button, count, delay_ms)?;
        self.send_command(&cmd)
    }

    pub fn mouse_turbo(
        &mut self,
        button: MouseButtons,
        delay_ms: u16,
    ) -> MakcuResult<String> {
        let cmd = MouseControl::build_turbo_command(button, delay_ms)?;
        self.send_command(&cmd)
    }

    pub fn mouse_disable_turbo(&mut self, button: MouseButtons) -> MakcuResult<String> {
        let cmd = MouseControl::build_disable_turbo_command(button);
        self.send_command(&cmd)
    }

    pub fn mouse_disable_all_turbo(&mut self) -> MakcuResult<String> {
        let cmd = MouseControl::build_disable_all_turbo_command();
        self.send_command(&cmd)
    }

    pub fn mouse_move(
        &mut self,
        dx: i16,
        dy: i16,
        segments: Option<u16>,
        control_points: Option<[(i16, i16); 2]>,
    ) -> MakcuResult<String> {
        let cmd = MouseControl::build_move_command(dx, dy, segments, control_points)?;
        self.send_command(&cmd)
    }

    pub fn mouse_moveto(
        &mut self,
        x: u16,
        y: u16,
        segments: Option<u16>,
        control_points: Option<[(i16, i16); 2]>,
    ) -> MakcuResult<String> {
        let cmd = MouseControl::build_moveto_command(x, y, segments, control_points)?;
        self.send_command(&cmd)
    }

    pub fn mouse_wheel(&mut self, delta: i8) -> MakcuResult<String> {
        let cmd = MouseControl::build_wheel_command(delta);
        self.send_command(&cmd)
    }

    pub fn mouse_pan(&mut self, steps: i16) -> MakcuResult<String> {
        let cmd = MouseControl::build_pan_command(steps);
        self.send_command(&cmd)
    }

    pub fn mouse_tilt(&mut self, steps: i16) -> MakcuResult<String> {
        let cmd = MouseControl::build_tilt_command(steps);
        self.send_command(&cmd)
    }

    pub fn mouse_getpos(&mut self) -> MakcuResult<String> {
        let cmd = MouseControl::build_getpos_command();
        self.send_command(&cmd)
    }

    pub fn mouse_silent(&mut self, x: u16, y: u16) -> MakcuResult<String> {
        let cmd = MouseControl::build_silent_command(x, y);
        self.send_command(&cmd)
    }

    pub fn keyboard_down(&mut self, key: Key) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_down_command(key);
        self.send_command(&cmd)
    }

    pub fn keyboard_up(&mut self, key: Key) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_up_command(key);
        self.send_command(&cmd)
    }

    pub fn keyboard_press(
        &mut self,
        key: Key,
        hold_ms: Option<u16>,
        rand_ms: Option<u8>,
    ) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_press_command(key, hold_ms, rand_ms)?;
        self.send_command(&cmd)
    }

    pub fn keyboard_string(&mut self, text: &str) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_string_command(text)?;
        self.send_command(&cmd)
    }

    pub fn keyboard_init(&mut self) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_init_command();
        self.send_command(&cmd)
    }

    pub fn keyboard_isdown(&mut self, key: Key) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_isdown_command(key);
        self.send_command(&cmd)
    }

    pub fn keyboard_disable(&mut self, keys: Vec<Key>) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_disable_command(keys);
        self.send_command(&cmd)
    }

    pub fn keyboard_enable(&mut self, key: Key) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_enable_command(key);
        self.send_command(&cmd)
    }

    pub fn keyboard_mask(&mut self, key: Key, mode: u8) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_mask_command(key, mode);
        self.send_command(&cmd)
    }

    pub fn keyboard_remap(&mut self, source: Key, target: Key) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_remap_command(source, target);
        self.send_command(&cmd)
    }

    pub fn keyboard_clear_remap(&mut self, key: Key) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_clear_remap_command(key);
        self.send_command(&cmd)
    }

    pub fn keyboard_reset_remap(&mut self) -> MakcuResult<String> {
        let cmd = KeyboardControl::build_reset_remap_command();
        self.send_command(&cmd)
    }

    pub fn led_query(&mut self, target: LedTarget) -> MakcuResult<String> {
        let cmd = LedControl::build_query_command(target);
        self.send_command(&cmd)
    }

    pub fn led_set(&mut self, target: LedTarget, mode: LedMode) -> MakcuResult<String> {
        let cmd = LedControl::build_set_command(target, mode);
        self.send_command(&cmd)
    }

    pub fn led_blink(
        &mut self,
        target: LedTarget,
        times: u8,
        delay_ms: u16,
    ) -> MakcuResult<String> {
        let cmd = LedControl::build_blink_command(target, times, delay_ms)?;
        self.send_command(&cmd)
    }

    pub fn stream_keyboard(&mut self, mode: u8, period: u16) -> MakcuResult<String> {
        let cmd = format!(".keyboard({},{})\r\n", mode, period);
        self.send_command(&cmd)
    }

    pub fn stream_buttons(&mut self, mode: u8, period_ms: u16) -> MakcuResult<String> {
        let cmd = format!(".buttons({},{})\r\n", mode, period_ms);
        self.send_command(&cmd)
    }

    pub fn stream_axis(&mut self, mode: u8, period_ms: u16) -> MakcuResult<String> {
        let cmd = format!(".axis({},{})\r\n", mode, period_ms);
        self.send_command(&cmd)
    }

    pub fn stream_mouse(&mut self, mode: u8, period_ms: u16) -> MakcuResult<String> {
        let cmd = format!(".mouse({},{})\r\n", mode, period_ms);
        self.send_command(&cmd)
    }
}

impl Drop for MakcuClient {
    fn drop(&mut self) {
        let _ = self.port.write_all(b".release()\r\n");
        let _ = self.port.flush();
    }
}
