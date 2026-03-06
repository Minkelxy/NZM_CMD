// src/scheduler.rs
use serde::Deserialize;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize, Debug, Clone)]
pub struct TaskSchedule {
    #[serde(rename = "type")]
    pub schedule_type: String,
    pub time: Option<String>,
    pub minutes: Option<u64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Task {
    pub name: String,
    pub target: String,
    pub enabled: bool,
    pub schedule: TaskSchedule,
}

#[derive(Deserialize, Debug)]
struct TasksRoot {
    tasks: Vec<Task>,
}

pub struct TaskScheduler {
    tasks: Vec<Task>,
    last_execution: std::collections::HashMap<String, u64>,
}

impl TaskScheduler {
    pub fn new(file_path: &str) -> Self {
        let tasks = if Path::new(file_path).exists() {
            match fs::read_to_string(file_path) {
                Ok(content) => {
                    match toml::from_str::<TasksRoot>(&content) {
                        Ok(root) => root.tasks,
                        Err(e) => {
                            println!("⚠️ 任务配置解析错误: {}", e);
                            Vec::new()
                        }
                    }
                }
                Err(e) => {
                    println!("⚠️ 无法读取任务配置: {}", e);
                    Vec::new()
                }
            }
        } else {
            println!("⚠️ 任务配置文件不存在: {}", file_path);
            Vec::new()
        };

        Self {
            tasks,
            last_execution: std::collections::HashMap::new(),
        }
    }

    pub fn list_tasks(&self) {
        println!("\n📋 任务列表:");
        println!("{}", "─".repeat(60));
        
        if self.tasks.is_empty() {
            println!("   (无任务)");
            return;
        }

        for (i, task) in self.tasks.iter().enumerate() {
            let status = if task.enabled { "✅" } else { "⬜" };
            let schedule_info = match task.schedule.schedule_type.as_str() {
                "daily" => format!("每日 {}", task.schedule.time.as_deref().unwrap_or("??:??")),
                "interval" => format!("每 {} 分钟", task.schedule.minutes.unwrap_or(0)),
                _ => "未知".to_string(),
            };
            
            println!("   {} [{}] {} -> {} ({})",
                status, i + 1, task.name, task.target, schedule_info);
        }
        
        println!("{}", "─".repeat(60));
    }

    pub fn get_due_tasks(&mut self) -> Vec<Task> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let today_start = (now / 86400) * 86400;
        let current_time_of_day = now % 86400;

        let mut due_tasks = Vec::new();

        for task in &self.tasks {
            if !task.enabled {
                continue;
            }

            let should_run = match task.schedule.schedule_type.as_str() {
                "daily" => {
                    if let Some(time_str) = &task.schedule.time {
                        if let Some((hour, minute)) = self.parse_time(time_str) {
                            let target_seconds = (hour as u64 * 3600) + (minute as u64 * 60);
                            let last_key = format!("{}_{}", task.name, today_start);
                            
                            let diff = if current_time_of_day >= target_seconds {
                                current_time_of_day - target_seconds
                            } else {
                                u64::MAX
                            };

                            diff < 300 && !self.last_execution.contains_key(&last_key)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                "interval" => {
                    let interval_secs = task.schedule.minutes.unwrap_or(60) * 60;
                    let last = self.last_execution.get(&task.name).copied().unwrap_or(0);
                    now - last >= interval_secs
                }
                _ => false,
            };

            if should_run {
                let key = match task.schedule.schedule_type.as_str() {
                    "daily" => format!("{}_{}", task.name, today_start),
                    _ => task.name.clone(),
                };
                self.last_execution.insert(key, now);
                due_tasks.push(task.clone());
            }
        }

        due_tasks
    }

    fn parse_time(&self, time_str: &str) -> Option<(u8, u8)> {
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() == 2 {
            let hour: u8 = parts[0].parse().ok()?;
            let minute: u8 = parts[1].parse().ok()?;
            if hour < 24 && minute < 60 {
                return Some((hour, minute));
            }
        }
        None
    }

    pub fn get_next_task_time(&self) -> Option<u64> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let current_time_of_day = now % 86400;
        let today_start = (now / 86400) * 86400;

        let mut next_time: Option<u64> = None;

        for task in &self.tasks {
            if !task.enabled {
                continue;
            }

            let task_next = match task.schedule.schedule_type.as_str() {
                "daily" => {
                    if let Some(time_str) = &task.schedule.time {
                        if let Some((hour, minute)) = self.parse_time(time_str) {
                            let target_seconds = (hour as u64 * 3600) + (minute as u64 * 60);
                            
                            if current_time_of_day < target_seconds {
                                Some(today_start + target_seconds)
                            } else {
                                Some(today_start + 86400 + target_seconds)
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                "interval" => {
                    let interval_secs = task.schedule.minutes.unwrap_or(60) * 60;
                    let last = self.last_execution.get(&task.name).copied().unwrap_or(now);
                    Some(last + interval_secs)
                }
                _ => None,
            };

            if let Some(t) = task_next {
                if next_time.is_none() || t < next_time.unwrap() {
                    next_time = Some(t);
                }
            }
        }

        next_time
    }

    pub fn format_next_time(&self) -> String {
        if let Some(next) = self.get_next_task_time() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let diff = next.saturating_sub(now);
            
            if diff < 60 {
                format!("{}秒后", diff)
            } else if diff < 3600 {
                format!("{}分钟后", diff / 60)
            } else if diff < 86400 {
                format!("{}小时{}分钟后", diff / 3600, (diff % 3600) / 60)
            } else {
                format!("{}天后", diff / 86400)
            }
        } else {
            "无计划任务".to_string()
        }
    }
}

use std::path::Path;
