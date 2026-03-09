use std::time::Duration;

#[derive(Debug, Clone)]
pub struct MakcuConfig {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
    pub screen_width: u16,
    pub screen_height: u16,
}

impl Default for MakcuConfig {
    fn default() -> Self {
        Self {
            port_name: "COM3".to_string(),
            baud_rate: 115200,
            timeout_ms: 100,
            screen_width: 1920,
            screen_height: 1080,
        }
    }
}

impl MakcuConfig {
    pub fn new(port_name: &str) -> Self {
        Self {
            port_name: port_name.to_string(),
            ..Default::default()
        }
    }

    pub fn with_baud_rate(mut self, baud_rate: u32) -> Self {
        self.baud_rate = baud_rate;
        self
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_screen_size(mut self, width: u16, height: u16) -> Self {
        self.screen_width = width;
        self.screen_height = height;
        self
    }

    pub fn timeout_duration(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}
