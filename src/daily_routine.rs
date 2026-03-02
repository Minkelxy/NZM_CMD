// src/daily_routine.rs
use crate::human::HumanDriver;
use crate::nav::NavEngine;
use crate::tower_defense::TowerDefenseApp;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DAILY_RECORD_FILE: &str = "daily_record.txt";

fn get_today_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let days = duration.as_secs() / 86400;
    format!("{}", days)
}

fn check_today_executed() -> bool {
    if !Path::new(DAILY_RECORD_FILE).exists() {
        return false;
    }
    if let Ok(content) = fs::read_to_string(DAILY_RECORD_FILE) {
        let today = get_today_string();
        return content.trim() == today;
    }
    false
}

fn mark_today_executed() {
    let today = get_today_string();
    let _ = fs::write(DAILY_RECORD_FILE, &today);
}

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
        
        if !check_today_executed() {
            println!("🎮 [Daily] 今日尚未执行塔防，先执行一次 [空间站英雄]...");
            self.run_tower_defense();
            mark_today_executed();
            println!("✅ [Daily] 塔防执行完成，继续每日任务...");
            thread::sleep(Duration::from_secs(3));
        } else {
            println!("✅ [Daily] 今日已执行过塔防，跳过。");
        }
        
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

    fn run_tower_defense(&self) {
        let mut td_app = TowerDefenseApp::new(
            Arc::clone(&self.driver),
            Arc::clone(&self.nav),
        );

        let scene_id = "空间站英雄";
        let map_dir = format!("maps/{}", scene_id);
        let map_file = format!("{}/{}地图.json", map_dir, scene_id);
        let strategy_file = format!("{}/{}策略.json", map_dir, scene_id);
        let traps_file = format!("{}/{}防御塔列表.json", map_dir, scene_id);

        println!("📂 加载塔防配置: {}", map_dir);
        td_app.run(&map_file, &strategy_file, &traps_file);
    }

    fn process_slot(&self, slot: &TaskSlot) -> bool {
        // 1. OCR 识别状态
        let text = self.nav.ocr_area(slot.status_rect);
        // 去除空格和换行，防止 OCR 识别出 "已 完 成" 导致匹配失败
        let clean_text = text.replace(|c: char| c.is_whitespace(), ""); 

        println!("   📝 槽位[{}] 识别结果: [{}]", slot.index, clean_text);

        // =========================================================
        // 逻辑判断 (注意顺序：先排除终态，再判断操作)
        // =========================================================

        // 1. 【终态】已完成 / 已领取
        // ⚠️ 必须放在最前面！因为 "已领取" 包含 "领取" 字样
        if clean_text.contains("已完成") || clean_text.contains("已领取") {
            println!("      -> ✅ 任务已结束，跳过。");
            return false; // 不做操作
        }

        // 2. 【可领取】
        if clean_text.contains("领取") {
            println!("      -> 🎉 发现可领取奖励，执行领取流程...");
            if let Ok(mut d) = self.driver.lock() {
                // A. 点击状态文字中心 (即领取按钮)
                let cx = (slot.status_rect[0] + slot.status_rect[2]) / 2;
                let cy = (slot.status_rect[1] + slot.status_rect[3]) / 2;
                d.move_to_humanly(cx as u16, cy as u16, 0.5);
                d.click_humanly(true, false, 0);

                // B. 处理奖励弹窗 (按空格跳过)
                println!("      -> ⏳ 等待弹窗并按空格跳过...");
                thread::sleep(Duration::from_millis(1000)); // 等待动画
                d.key_click(' '); 
                thread::sleep(Duration::from_millis(1000));
                d.key_click(' '); // 连按两次防止漏掉
            }
            return true; // 做了操作，需要重试扫描
        }

        // 3. 【未完成】需要刷新
        if clean_text.contains("去完成") || clean_text.contains("未完成") {
            println!("      -> ⚠️ 任务未完成，点击刷新 ({}, {})...", slot.refresh_pos.0, slot.refresh_pos.1);
            if let Ok(mut d) = self.driver.lock() {
                // 点击对应的刷新按钮
                d.move_to_humanly(slot.refresh_pos.0, slot.refresh_pos.1, 0.5);
                d.click_humanly(true, false, 0);
                
                // 刷新后的短暂冷却
                thread::sleep(Duration::from_millis(500));
            }
            return true; // 做了操作，需要重试扫描
        }
        
        // 4. 【兜底】识别为空或其他未知状态
        if clean_text.is_empty() {
             println!("      -> ⚪ 识别为空 (可能是图标/过暗)，暂跳过");
             return false;
        }

        println!("      -> ❓ 未知状态，跳过");
        false
    }
}