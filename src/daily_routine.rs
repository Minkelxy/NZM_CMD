// src/daily_routine.rs
use crate::human::HumanDriver;
use crate::nav::NavEngine;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct TaskSlot {
    index: usize,
    status_rect: [i32; 4],
    refresh_pos: (u16, u16),
}

pub struct DailyRoutineApp {
    driver: Arc<Mutex<HumanDriver>>,
    nav: Arc<NavEngine>,
    slots: Vec<TaskSlot>,
}

impl DailyRoutineApp {
    pub fn new(driver: Arc<Mutex<HumanDriver>>, nav: Arc<NavEngine>) -> Self {
        let slots = vec![
            TaskSlot {
                index: 1,
                status_rect: [559, 914, 768, 963],
                refresh_pos: (784, 311),
            },
            TaskSlot {
                index: 2,
                status_rect: [899, 901, 1104, 977],
                refresh_pos: (1124, 314),
            },
            TaskSlot {
                index: 3,
                status_rect: [1238, 901, 1439, 968],
                refresh_pos: (1465, 318),
            },
            TaskSlot {
                index: 4,
                status_rect: [1560, 895, 1792, 968],
                refresh_pos: (1804, 316),
            },
        ];

        Self { driver, nav, slots }
    }

    pub fn run(&self) {
        println!("📅 [Daily] 开始执行日活任务逻辑...");
        
        let max_rounds = 10; 

        for round in 1..=max_rounds {
            println!("\n🔄 [Daily] 第 {}/{} 轮扫描...", round, max_rounds);
            
            let mut need_retry = false;
            
            for slot in &self.slots {
                let processed = self.process_slot(slot);
                if processed {
                    need_retry = true;
                }
                thread::sleep(Duration::from_millis(500)); 
            }

            if !need_retry {
                println!("✅ [Daily] 所有任务已完成或已领取！");
                break;
            }

            println!("⏳ 等待任务列表刷新 (2秒)...");
            thread::sleep(Duration::from_secs(2));
        }

        println!("🏁 [Daily] 日活流程结束。");
    }

    fn process_slot(&self, slot: &TaskSlot) -> bool {
        let text = self.nav.ocr_area(slot.status_rect);
        let clean_text = text.replace(|c: char| c.is_whitespace(), ""); 

        println!("   📝 槽位[{}] 识别结果: [{}]", slot.index, clean_text);

        if clean_text.contains("已完成") || clean_text.contains("已领取") {
            println!("      -> ✅ 任务已结束，跳过。");
            return false;
        }

        if clean_text.contains("领取") {
            println!("      -> 🎉 发现可领取奖励，执行领取流程...");
            if let Ok(mut d) = self.driver.lock() {
                let cx = (slot.status_rect[0] + slot.status_rect[2]) / 2;
                let cy = (slot.status_rect[1] + slot.status_rect[3]) / 2;
                d.move_to_humanly(cx as u16, cy as u16, 0.5);
                d.click_humanly(true, false, 0);

                println!("      -> ⏳ 等待弹窗并按空格跳过...");
                thread::sleep(Duration::from_millis(1000));
                d.key_click(' '); 
                thread::sleep(Duration::from_millis(1000));
                d.key_click(' ');
            }
            return true;
        }

        if clean_text.contains("去完成") || clean_text.contains("未完成") {
            println!("      -> ⚠️ 任务未完成，点击刷新 ({}, {})...", slot.refresh_pos.0, slot.refresh_pos.1);
            if let Ok(mut d) = self.driver.lock() {
                d.move_to_humanly(slot.refresh_pos.0, slot.refresh_pos.1, 0.5);
                d.click_humanly(true, false, 0);
                thread::sleep(Duration::from_millis(500));
            }
            return true;
        }
        
        if clean_text.is_empty() {
             println!("      -> ⚪ 识别为空，暂跳过");
             return false;
        }

        println!("      -> ❓ 未知状态，跳过");
        false
    }
}
