// src/lib.rs

pub mod hardware;      // 新增：底层驱动
pub mod human;         // 拟人化层
pub mod nav;           // 视觉导航层
pub mod tower_defense; // 业务逻辑层
pub mod daily_routine; // 日常任务层
pub mod scheduler;     // 任务调度层