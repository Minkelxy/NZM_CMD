use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum MakcuError {
    SerialPortError(String),
    ParseError(String),
    TimeoutError,
    DeviceNotConnected,
    InvalidParameter(String),
    CommandFailed(String),
}

impl fmt::Display for MakcuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MakcuError::SerialPortError(msg) => write!(f, "串口错误: {}", msg),
            MakcuError::ParseError(msg) => write!(f, "解析错误: {}", msg),
            MakcuError::TimeoutError => write!(f, "操作超时"),
            MakcuError::DeviceNotConnected => write!(f, "设备未连接"),
            MakcuError::InvalidParameter(msg) => write!(f, "无效参数: {}", msg),
            MakcuError::CommandFailed(msg) => write!(f, "命令执行失败: {}", msg),
        }
    }
}

impl std::error::Error for MakcuError {}

pub type MakcuResult<T> = Result<T, MakcuError>;
