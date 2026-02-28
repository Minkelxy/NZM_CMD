use crate::human::HumanDriver;
use crate::nav::NavEngine;
use regex::Regex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ==========================================
// 1. 数据结构协议
// ==========================================

// ✨ 新增：预备阶段动作定义 (用于 MapMeta)
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum PrepAction {
    KeyDown { key: char },
    KeyUpAll,
    Wait { ms: u64 },
    Log { msg: String },
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum InitAction {
    Move {
        x: u16,
        y: u16,
    },
    Click {
        #[serde(default)]
        left: bool,
        #[serde(default)]
        right: bool,
        #[serde(default)]
        hold_ms: u64,
    },
    Key {
        char: char,
    },
    Wait {
        ms: u64,
    },
    Log {
        msg: String,
    },
}

#[derive(Debug, Clone)]
pub struct TDConfig {
    pub hud_check_rect: [i32; 4],
    pub hud_wave_loop_rect: [i32; 4],
    pub safe_zone: [i32; 4],
    pub screen_width: f32,
    pub screen_height: f32,
}

impl Default for TDConfig {
    fn default() -> Self {
        Self {
            hud_check_rect: [262, 16, 389, 97],
            hud_wave_loop_rect: [344, 290, 548, 353],
            safe_zone: [200, 200, 1720, 880],
            screen_width: 1920.0,
            screen_height: 1080.0,
        }
    }
}

// ✨ 修改：TrapConfigItem 增加 b_type 和 grid_index
#[derive(Deserialize, Debug, Clone)]
pub struct TrapConfigItem {
    pub name: String,
    #[serde(default)]
    pub b_type: String, // "Floor", "Wall", "Ceiling"
    #[serde(default)]
    pub grid_index: [i32; 2], // [col, row]
}

// ✨ 修改：MapMeta 增加 prep_actions
#[derive(Deserialize, Debug, Clone)]
pub struct ViewportSafeArea {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MapMeta {
    pub grid_pixel_width: f32,
    pub grid_pixel_height: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub bottom: f32,
    #[serde(default)]
    pub right: f32,
    #[serde(default = "default_camera_speed_up")]
    pub camera_speed_up: f32,
    #[serde(default = "default_camera_speed_down")]
    pub camera_speed_down: f32,
    #[serde(default = "default_camera_speed_left")]
    pub camera_speed_left: f32,
    #[serde(default = "default_camera_speed_right")]
    pub camera_speed_right: f32,
    #[serde(default)]
    pub viewport_safe_areas: Vec<ViewportSafeArea>,
    #[serde(default)]
    pub prep_actions: Vec<PrepAction>,
}

fn default_camera_speed_up() -> f32 { 300.0 }
fn default_camera_speed_down() -> f32 { 300.0 }
fn default_camera_speed_left() -> f32 { 300.0 }
fn default_camera_speed_right() -> f32 { 300.0 }

#[derive(Deserialize, Debug, Clone)]
pub struct BuildingExport {
    pub uid: usize,
    pub name: String,
    pub grid_x: usize,
    pub grid_y: usize,
    pub width: usize,
    pub height: usize,
    #[serde(default)]
    pub wave_num: i32,
    #[serde(default)]
    pub is_late: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UpgradeEvent {
    pub building_name: String,
    pub wave_num: i32,
    pub is_late: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DemolishEvent {
    pub uid: usize,
    pub name: String,
    pub grid_x: usize,
    pub grid_y: usize,
    pub width: usize,
    pub height: usize,
    pub wave_num: i32,
    pub is_late: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MapTerrainExport {
    pub map_name: String,
    pub meta: MapMeta,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MapBuildingsExport {
    pub map_name: String,
    pub buildings: Vec<BuildingExport>,
    #[serde(default)]
    pub upgrades: Vec<UpgradeEvent>,
    #[serde(default)]
    pub demolishes: Vec<DemolishEvent>,
}

#[derive(Debug, Default)]
pub struct WaveStatus {
    pub current_wave: i32,
}

#[derive(Clone)]
enum TaskAction {
    Demolish(DemolishEvent),
    Place(BuildingExport),
    Upgrade(UpgradeEvent),
}

#[derive(Clone)]
struct ScheduledTask {
    action: TaskAction,
    map_y: f32,
    map_x: f32,
    priority: u8,
}

// 辅助函数：将字符转换为 HID 键码
fn get_hid_code(c: char) -> u8 {
    match c.to_ascii_lowercase() {
        'a'..='z' => c.to_ascii_lowercase() as u8 - b'a' + 0x04,
        '0'..='9' => c as u8 - b'1' + 0x1E,
        ' ' => 0x2C,
        _ => 0,
    }
}

// ==========================================
// 2. 塔防模块实现
// ==========================================
#[derive(Clone)]
pub struct TowerDefenseApp {
    driver: Arc<Mutex<HumanDriver>>,
    nav: Arc<NavEngine>,
    config: TDConfig,
    map_meta: Option<MapMeta>,

    strategy_buildings: Vec<BuildingExport>,
    strategy_upgrades: Vec<UpgradeEvent>,
    strategy_demolishes: Vec<DemolishEvent>,

    placed_uids: HashSet<usize>,
    completed_upgrade_keys: HashSet<String>,
    completed_demolish_uids: HashSet<usize>,

    last_confirmed_wave: i32,
    last_wave_change_time: Instant,
    last_ocr_text: String,

    trap_lookup: HashMap<String, TrapConfigItem>,
    active_loadout: Vec<String>,

    camera_offset_x: f32,
    camera_offset_y: f32,
}

impl TowerDefenseApp {
    pub fn new(driver: Arc<Mutex<HumanDriver>>, nav: Arc<NavEngine>) -> Self {
        Self {
            driver,
            nav,
            config: TDConfig::default(),
            map_meta: None,
            strategy_buildings: Vec::new(),
            strategy_upgrades: Vec::new(),
            strategy_demolishes: Vec::new(),
            placed_uids: HashSet::new(),
            completed_upgrade_keys: HashSet::new(),
            completed_demolish_uids: HashSet::new(),
            last_confirmed_wave: 0,
            last_wave_change_time: Instant::now(),
            last_ocr_text: String::new(),
            trap_lookup: HashMap::new(),
            active_loadout: Vec::new(),
            camera_offset_x: 0.0,
            camera_offset_y: 0.0,
        }
    }

    pub fn load_strategy(&mut self, path: &str) {
        if let Ok(c) = fs::read_to_string(path) {
            if let Ok(data) = serde_json::from_str::<MapBuildingsExport>(&c) {
                self.strategy_buildings = data.buildings;
                self.strategy_upgrades = data.upgrades;
                self.strategy_demolishes = data.demolishes;
                println!(
                    "🏗️ 策略加载成功: 建{} | 升{} | 拆{}",
                    self.strategy_buildings.len(),
                    self.strategy_upgrades.len(),
                    self.strategy_demolishes.len()
                );
            } else {
                println!("❌ 策略 JSON 解析失败");
            }
        }
    }

    fn extract_wave_with_validation(&self, text: &str, is_same_text: bool) -> Option<i32> {
        if is_same_text {
            println!("🔧 [OCR Smart] 文本相同，保持波次: {}", self.last_confirmed_wave);
            return Some(self.last_confirmed_wave);
        }
        
        let re = Regex::new(r"(\d+)/\d+.*波次").ok()?;
        if let Some(caps) = re.captures(text) {
            if let Ok(num) = caps.get(1)?.as_str().parse::<i32>() {
                if num == self.last_confirmed_wave || num == self.last_confirmed_wave + 1 {
                    println!("🔧 [OCR Smart] 格式匹配: {} (文本: {})", num, text.trim());
                    return Some(num);
                }
            }
        }
        
        let re_all = Regex::new(r"\d+").ok()?;
        let candidates: Vec<i32> = re_all
            .captures_iter(text)
            .filter_map(|c| c.get(0)?.as_str().parse().ok())
            .filter(|&n| n > 0 && n <= 100)
            .collect();
        
        if !candidates.is_empty() {
            let valid: Vec<i32> = candidates
                .iter()
                .filter(|&&n| n == self.last_confirmed_wave || n == self.last_confirmed_wave + 1)
                .cloned()
                .collect();
            
            if !valid.is_empty() {
                let valid_sorted: Vec<i32> = {
                    let mut v = valid.clone();
                    v.sort();
                    v
                };
                if let Some(&num) = valid_sorted.first() {
                    println!("🔧 [OCR Smart] 选择有效波次: {} (候选: {:?})", num, valid_sorted);
                    return Some(num);
                }
            }
            
            if let Some(&num) = candidates.first() {
                println!("🔧 [OCR Smart] 无有效波次，选择第一个: {}", num);
                return Some(num);
            }
        }
        
        if text.contains("波次") {
            let inferred = self.last_confirmed_wave + 1;
            println!("🔧 [OCR Smart] 文本含「波次」但无数字，推断为: {}", inferred);
            return Some(inferred);
        }
        
        None
    }

    pub fn recognize_wave_status(&mut self, rect: [i32; 4], use_tab: bool) -> Option<WaveStatus> {
        const KEY_TAB: u8 = 0x2B;
        if use_tab {
            if let Ok(driver) = self.driver.lock() {
                if let Ok(mut dev) = driver.device.lock() {
                    dev.key_down(KEY_TAB, 0);
                }
            }
            thread::sleep(Duration::from_millis(500));
        }

        let text: String = self.nav.ocr_area(rect);

        if use_tab {
            if let Ok(driver) = self.driver.lock() {
                if let Ok(mut dev) = driver.device.lock() {
                    dev.key_up();
                }
            }
            thread::sleep(Duration::from_millis(500));
            if let Ok(driver) = self.driver.lock() {
                if let Ok(mut dev) = driver.device.lock() {
                    dev.key_down(KEY_TAB, 0);
                }
            }
            thread::sleep(Duration::from_millis(100));
            if let Ok(driver) = self.driver.lock() {
                if let Ok(mut dev) = driver.device.lock() {
                    dev.key_up();
                }
            }
        }

        if text.is_empty() {
            return None;
        }

        println!(
            "🔍 [OCR Debug] 原始文本: 「{}」 (Mode: {})",
            text.trim(),
            if use_tab { "TAB" } else { "HUD" }
        );

        let is_same_text = self.last_ocr_text == text;
        self.last_ocr_text = text.clone();
        
        let val = self.extract_wave_with_validation(&text, is_same_text)?;
        println!("✅ [OCR Match] 波次识别成功: 第 {} 波", val);
        Some(WaveStatus { current_wave: val })
    }

    fn validate_wave_transition(&mut self, detected_wave: i32) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_wave_change_time).as_secs();
        let is_next_wave = detected_wave == self.last_confirmed_wave + 1;
        let is_long_enough = elapsed >= 60 || self.last_confirmed_wave == 0;
        if is_next_wave && is_long_enough {
            println!(
                "✅ [Monitor] 新波次: {} -> {}",
                self.last_confirmed_wave, detected_wave
            );
            self.last_confirmed_wave = detected_wave;
            self.last_wave_change_time = now;
            true
        } else {
            false
        }
    }

    fn are_tasks_in_current_view(&self, tasks: &[ScheduledTask]) -> bool {
        let [sz_x1, sz_y1, sz_x2, sz_y2] = self.config.safe_zone;
        let view_left = self.camera_offset_x;
        let view_top = self.camera_offset_y;
        let safe_map_left = view_left + sz_x1 as f32;
        let safe_map_right = view_left + sz_x2 as f32;
        let safe_map_top = view_top + sz_y1 as f32;
        let safe_map_bottom = view_top + sz_y2 as f32;

        println!("📍 [视野检查] 摄像机位置: ({:.0}, {:.0})", view_left, view_top);
        println!("📍 [视野检查] 安全区范围: X[{:.0}-{:.0}] Y[{:.0}-{:.0}]", 
            safe_map_left, safe_map_right, safe_map_top, safe_map_bottom);

        let mut has_positional_task = false;
        for task in tasks {
            if matches!(task.action, TaskAction::Upgrade(_)) {
                continue;
            }
            has_positional_task = true;
            println!("📍 [视野检查] 任务位置: ({:.0}, {:.0})", task.map_x, task.map_y);
            if task.map_x < safe_map_left || task.map_x > safe_map_right ||
               task.map_y < safe_map_top || task.map_y > safe_map_bottom {
                println!("❌ [视野检查] 任务不在安全区内");
                return false;
            }
        }
        if has_positional_task {
            println!("✅ [视野检查] 所有任务在安全区内");
        } else {
            println!("⚠️ [视野检查] 没有位置任务（仅升级任务）");
        }
        has_positional_task
    }

    pub fn execute_wave_phase(&mut self, wave: i32, is_late: bool) {
        let phase_name = if is_late { "后期" } else { "前期" };
        println!(
            "🚀 优化执行第 {} 波 [{}] (拆除优先模式)...",
            wave, phase_name
        );
        
        if !is_late {
            self.calibrate_camera_to_corner(true);
        }

        let mut demolish_tasks = Vec::new();
        let mut build_upgrade_tasks = Vec::new();

        for d in self.strategy_demolishes.iter().filter(|d| {
            d.wave_num == wave
                && d.is_late == is_late
                && !self.completed_demolish_uids.contains(&d.uid)
        }) {
            if let Some((px, py)) =
                self.get_absolute_map_pixel(d.grid_x, d.grid_y, d.width, d.height)
            {
                demolish_tasks.push(ScheduledTask {
                    action: TaskAction::Demolish(d.clone()),
                    map_y: py,
                    map_x: px,
                    priority: 0,
                });
            }
        }

        for b in self.strategy_buildings.iter().filter(|b| {
            b.wave_num == wave && b.is_late == is_late && !self.placed_uids.contains(&b.uid)
        }) {
            if let Some((px, py)) =
                self.get_absolute_map_pixel(b.grid_x, b.grid_y, b.width, b.height)
            {
                build_upgrade_tasks.push(ScheduledTask {
                    action: TaskAction::Place(b.clone()),
                    map_y: py,
                    map_x: px,
                    priority: 1,
                });
            }
        }

        for u in self
            .strategy_upgrades
            .iter()
            .filter(|u| u.wave_num == wave && u.is_late == is_late)
        {
            let key = format!("{}-{}-{}", u.building_name, u.wave_num, u.is_late);
            if !self.completed_upgrade_keys.contains(&key) {
                build_upgrade_tasks.push(ScheduledTask {
                    action: TaskAction::Upgrade(u.clone()),
                    map_y: 0.0,
                    map_x: 0.0,
                    priority: 2,
                });
            }
        }

        if demolish_tasks.is_empty() && build_upgrade_tasks.is_empty() {
            return;
        }

        if !demolish_tasks.is_empty() {
            println!(
                "🧹 [Step 1] 正在执行全图拆除任务 ({}个)...",
                demolish_tasks.len()
            );
            self.dispatch_tasks_by_region(demolish_tasks);
        }

        if !build_upgrade_tasks.is_empty() {
            println!(
                "🏗️ [Step 2] 正在执行建造与升级任务 ({}个)...",
                build_upgrade_tasks.len()
            );
            build_upgrade_tasks.sort_by(|a, b| a.priority.cmp(&b.priority));
            self.dispatch_tasks_by_region(build_upgrade_tasks);
        }
    }

    fn dispatch_tasks_by_region(&mut self, tasks: Vec<ScheduledTask>) {
        let meta = self.map_meta.as_ref().unwrap();
        let map_h = meta.bottom;
        let screen_h = self.config.screen_height;
        let mid_point = (map_h - screen_h) / 2.0;

        let (mut upper, mut lower): (Vec<_>, Vec<_>) = tasks
            .into_iter()
            .partition(|t| t.map_y <= mid_point + screen_h / 2.0);

        if !upper.is_empty() {
            upper.sort_by(|a, b| {
                a.map_y
                    .partial_cmp(&b.map_y)
                    .unwrap()
                    .then(a.priority.cmp(&b.priority))
            });
            if self.are_tasks_in_current_view(&upper) {
                println!("✨ 上半区任务在视野内，直接执行");
                self.process_task_batch(upper, false);
            } else {
                self.align_camera_to_edge(true);
                self.process_task_batch(upper, true);
            }
        }

        if !lower.is_empty() {
            lower.sort_by(|a, b| {
                b.map_y
                    .partial_cmp(&a.map_y)
                    .unwrap()
                    .then(a.priority.cmp(&b.priority))
            });
            if self.are_tasks_in_current_view(&lower) {
                println!("✨ 下半区任务在视野内，直接执行");
                self.process_task_batch(lower, false);
            } else {
                self.align_camera_to_edge(false);
                self.process_task_batch(lower, true);
            }
        }
    }

    fn process_task_batch(&mut self, tasks: Vec<ScheduledTask>, force_initial_refresh: bool) {
        let mut last_build_key: Option<char> = None;
        let mut is_first_task = true;

        for task in tasks {
            if let TaskAction::Upgrade(u) = &task.action {
                self.execute_single_upgrade(u);
                continue;
            }

            let [sz_x1, sz_y1, sz_x2, sz_y2] = self.config.safe_zone;
            let view_left = self.camera_offset_x;
            let view_top = self.camera_offset_y;
            let safe_map_left = view_left + sz_x1 as f32;
            let safe_map_right = view_left + sz_x2 as f32;
            let safe_map_top = view_top + sz_y1 as f32;
            let safe_map_bottom = view_top + sz_y2 as f32;

            let is_task_in_safe_zone = task.map_x >= safe_map_left && task.map_x <= safe_map_right &&
                                       task.map_y >= safe_map_top && task.map_y <= safe_map_bottom;

            let mut screen_moved = false;
            if !force_initial_refresh && is_task_in_safe_zone {
                println!("✨ 任务在安全区内，跳过摄像头移动");
            } else {
                screen_moved = self.smart_move_camera(task.map_x, task.map_y);
                if is_first_task && force_initial_refresh {
                    screen_moved = true;
                    is_first_task = false;
                }
            }

            match &task.action {
                TaskAction::Demolish(d) => {
                    self.perform_demolish_action(task.map_x, task.map_y, d.uid)
                }
                TaskAction::Place(b) => self.perform_build_action(
                    &mut last_build_key,
                    screen_moved,
                    task.map_x,
                    task.map_y,
                    &b.name,
                    b.uid,
                ),
                _ => {}
            }
        }
    }

// src/tower_defense.rs

    fn perform_demolish_action(&mut self, map_x: f32, map_y: f32, uid: usize) {
        let [sz_x1, sz_y1, sz_x2, sz_y2] = self.config.safe_zone;
        let screen_x = (map_x - self.camera_offset_x).clamp(sz_x1 as f32, sz_x2 as f32);
        let screen_y = (map_y - self.camera_offset_y).clamp(sz_y1 as f32, sz_y2 as f32);

        if let Ok(mut driver) = self.driver.lock() {
            // 1. 移动到位后强制停顿，确保准星彻底对齐格子
            driver.move_to_humanly(screen_x as u16, screen_y as u16, 0.4);
            thread::sleep(Duration::from_millis(50));

            // 2. 点击选中 (增加 hold 时间到 60ms，防止点击过快游戏未响应)
            driver.click_humanly(true, false, 60); 
            
            // 3. 等待选中框出现的延迟 (从 150ms 增加到 250ms)
            thread::sleep(Duration::from_millis(150));

            // 4. 🔥 双击 'E' 拆除 (Double Tap)
            // 第一下 E：执行拆除
            driver.key_click('e');
            
            // 间隔 100ms
            thread::sleep(Duration::from_millis(100));
            
            // 第二下 E：保险措施 (防止第一下被吞，或者部分陷阱需要二次确认)
            driver.key_click('e');
        }
        
        self.completed_demolish_uids.insert(uid);
        
        // 动作后摇 (稍微缩短一点，因为我们已经多按了一次E)
        thread::sleep(Duration::from_millis(200));
    }

// src/tower_defense.rs

    fn perform_build_action(
        &mut self,
        last_key: &mut Option<char>,
        screen_moved: bool,
        map_x: f32,
        map_y: f32,
        name: &str,
        uid: usize,
    ) {
        let [sz_x1, sz_y1, sz_x2, sz_y2] = self.config.safe_zone;
        let screen_x = (map_x - self.camera_offset_x).clamp(sz_x1 as f32, sz_x2 as f32);
        let screen_y = (map_y - self.camera_offset_y).clamp(sz_y1 as f32, sz_y2 as f32);
        let key = self.get_trap_key(name);

        if let Ok(mut d) = self.driver.lock() {
            // 1. 移动鼠标
            d.move_to_humanly(screen_x as u16, screen_y as u16, 0.35);

            // [稳定性] 移动到位后强制停顿，等待鼠标“落稳”
            thread::sleep(Duration::from_millis(50));

            // 🔥 [核心修复] 判定条件增加 `last_key.is_none()`
            // 含义：如果是本批次的第一座塔（无论是否移动了视野），或者刚刚移动过视野，
            // 都强制执行“三连击”切枪逻辑，确保陷阱切出率 100%。
            if screen_moved || last_key.is_none() {
                let swap_key = if key == '4' { '5' } else { '4' };
                
                // 执行：目标键 -> 干扰键 -> 目标键 (强刷状态)
                d.key_click(key);
                thread::sleep(Duration::from_millis(120));
                d.key_click(swap_key);
                thread::sleep(Duration::from_millis(120));
                d.key_click(key);

                // 等待陷阱虚影完全浮现
                thread::sleep(Duration::from_millis(250));
                *last_key = Some(key);
            } else if Some(key) != *last_key {
                // 如果不是第一座，且类型变了（原地换塔），则单次按键切换
                d.key_click(key);
                *last_key = Some(key);
                thread::sleep(Duration::from_millis(250));
            } else {
                // 同种塔连续放置，仅需微小延迟
                thread::sleep(Duration::from_millis(50));
            }

            // 执行双击放置
            d.double_click_humanly(true, false, 150);
        }
        self.placed_uids.insert(uid);

        // 动作后摇
        thread::sleep(Duration::from_millis(250));
    }

    fn execute_single_upgrade(&mut self, u: &UpgradeEvent) {
        let key = self.get_trap_key(&u.building_name);
        if let Ok(mut d) = self.driver.lock() {
            println!("   -> 长按 '{}' (800ms) 以升级: {}", key, u.building_name);
            d.key_hold(key, 1500);
        }
        let key_str = format!("{}-{}-{}", u.building_name, u.wave_num, u.is_late);
        self.completed_upgrade_keys.insert(key_str);
        thread::sleep(Duration::from_millis(400));
    }

    fn align_camera_to_edge(&mut self, top: bool) {
        let meta = self.map_meta.as_ref().unwrap();
        let max_scroll_y = (meta.bottom - self.config.screen_height).max(0.0);

        if let Ok(mut human) = self.driver.lock() {
            let key = if top { 'w' } else { 's' };
            println!("🔄 强制归零: {}", if top { "顶部" } else { "底部" });
            human.key_hold(key, 3500);
        }
        self.camera_offset_y = if top { 0.0 } else { max_scroll_y };
        self.clamp_camera_position();
        thread::sleep(Duration::from_millis(500));
    }

    fn calibrate_camera_to_corner(&mut self, top_left: bool) {
        let (min_x, _max_x, min_y, max_y) = self.get_camera_bounds();
        
        let target_x = min_x;
        let target_y = if top_left { min_y } else { max_y };
        
        println!("🎯 校准摄像机到 {}...", if top_left { "左上角" } else { "左下角" });
        
        if let Ok(mut human) = self.driver.lock() {
            human.key_hold('w', 3000);
            human.key_hold('a', 3000);
            if !top_left {
                human.key_hold('s', 3000);
            }
        }
        
        self.camera_offset_x = target_x;
        self.camera_offset_y = target_y;
        self.clamp_camera_position();
        thread::sleep(Duration::from_millis(500));
    }

    fn scroll_camera_by_pixels(
        &mut self,
        direction: char,
        pixels: f32,
        time_resolution_ms: u64,
    ) -> f32 {
        if pixels < 10.0 {
            return 0.0;
        }
        
        let speed = self.get_camera_speed(direction);
        let raw_ms = (pixels / speed * 1000.0) as u64;
        let units = (raw_ms + time_resolution_ms / 2) / time_resolution_ms;
        let final_ms = units.max(1) * time_resolution_ms;

        if let Ok(mut human) = self.driver.lock() {
            human.key_hold(direction, final_ms);
        }
        
        let moved = (final_ms as f32 / 1000.0) * speed;
        
        match direction {
            'w' => self.camera_offset_y -= moved,
            's' => self.camera_offset_y += moved,
            'a' => self.camera_offset_x -= moved,
            'd' => self.camera_offset_x += moved,
            _ => {}
        }
        
        self.clamp_camera_position();
        moved
    }
    
    fn move_camera_to_position(&mut self, target_x: f32, target_y: f32) -> bool {
        let (min_x, max_x, min_y, max_y) = self.get_camera_bounds();
        let target_x = target_x.clamp(min_x, max_x);
        let target_y = target_y.clamp(min_y, max_y);
        
        let delta_x = target_x - self.camera_offset_x;
        let delta_y = target_y - self.camera_offset_y;
        
        let mut moved = false;
        const SCROLL_RES: u64 = 100;
        
        if delta_x.abs() > 10.0 {
            let direction = if delta_x > 0.0 { 'd' } else { 'a' };
            self.scroll_camera_by_pixels(direction, delta_x.abs(), SCROLL_RES);
            moved = true;
        }
        
        if delta_y.abs() > 10.0 {
            let direction = if delta_y > 0.0 { 's' } else { 'w' };
            self.scroll_camera_by_pixels(direction, delta_y.abs(), SCROLL_RES);
            moved = true;
        }
        
        if moved {
            thread::sleep(Duration::from_millis(200));
        }
        
        moved
    }

    fn smart_move_camera(&mut self, target_map_x: f32, target_map_y: f32) -> bool {
        let [sz_x1, sz_y1, sz_x2, sz_y2] = self.config.safe_zone;
        let safe_center_screen_x = (sz_x1 + sz_x2) as f32 / 2.0;
        let safe_center_screen_y = (sz_y1 + sz_y2) as f32 / 2.0;
        
        let (min_x, max_x, min_y, max_y) = self.get_camera_bounds();
        
        let ideal_cam_x = (target_map_x - safe_center_screen_x).clamp(min_x, max_x);
        let ideal_cam_y = (target_map_y - safe_center_screen_y).clamp(min_y, max_y);
        
        let delta_x = ideal_cam_x - self.camera_offset_x;
        let delta_y = ideal_cam_y - self.camera_offset_y;

        const MOVE_THRESHOLD: f32 = 100.0;
        if delta_x.abs() < MOVE_THRESHOLD && delta_y.abs() < MOVE_THRESHOLD {
            return false;
        }
        
        let moved = self.move_camera_to_position(ideal_cam_x, ideal_cam_y);
        
        if moved {
            println!("📷 摄像机移动: ({:.0}, {:.0}) -> ({:.0}, {:.0})", 
                self.camera_offset_x, self.camera_offset_y, ideal_cam_x, ideal_cam_y);
        }
        
        moved
    }

    pub fn load_map_terrain(&mut self, path: &str) {
        if let Ok(c) = fs::read_to_string(path) {
            if let Ok(data) = serde_json::from_str::<MapTerrainExport>(&c) {
                self.map_meta = Some(data.meta);
            }
        }
    }

    pub fn load_trap_config(&mut self, json_path: &str) {
        if let Ok(c) = fs::read_to_string(json_path) {
            if let Ok(items) = serde_json::from_str::<Vec<TrapConfigItem>>(&c) {
                for item in items {
                    self.trap_lookup.insert(item.name.clone(), item);
                }
            }
        }
    }

    pub fn setup_view(&mut self) {
        println!("🔭 对齐左上角边界...");
        if let Ok(mut human) = self.driver.lock() {
            human.key_click('o');
            thread::sleep(Duration::from_secs(2));
            for _ in 1..=4 {
                for _ in 0..10 {
                    human.mouse_scroll(-120);
                    thread::sleep(Duration::from_millis(30));
                }
                thread::sleep(Duration::from_millis(100));
            }
            for _ in 1..=2 {
                human.key_hold('w', 200);
                thread::sleep(Duration::from_millis(50));
                human.key_hold('a', 200);
                thread::sleep(Duration::from_millis(50));
            }
            human.key_hold('w', 200);
            human.key_hold('a', 200);
        }
        self.camera_offset_x = 0.0;
        self.camera_offset_y = 0.0;
        self.clamp_camera_position();
        println!("✅ 摄像机已重置到 (0, 0)");
    }

    pub fn execute_prep_logic(&self) {
        println!("🔧 执行赛前准备...");

        if let Some(meta) = &self.map_meta {
            if !meta.prep_actions.is_empty() {
                println!("   -> 加载自定义战术动作 ({} 步)", meta.prep_actions.len());
                if let Ok(human) = self.driver.lock() {
                    if let Ok(mut dev) = human.device.lock() {
                        for action in &meta.prep_actions {
                            match action {
                                PrepAction::KeyDown { key } => {
                                    let code = get_hid_code(*key);
                                    if code != 0 {
                                        dev.key_down(code, 0);
                                    }
                                }
                                PrepAction::KeyUpAll => {
                                    dev.key_up();
                                }
                                PrepAction::Wait { ms } => {
                                    drop(dev);
                                    thread::sleep(Duration::from_millis(*ms));
                                    dev = human.device.lock().unwrap();
                                }
                                PrepAction::Log { msg } => {
                                    println!("   [Prep] {}", msg);
                                }
                            }
                        }
                        dev.key_up();
                    }
                }
            }
        }

        if let Ok(mut human) = self.driver.lock() {
            human.key_click('n');
            thread::sleep(Duration::from_millis(500));
        }

        self.select_loadout();

        if let Ok(mut human) = self.driver.lock() {
            human.key_click('n');
            thread::sleep(Duration::from_millis(500));
        }
    }

    pub fn select_loadout(&self) {
        const GRID_START_X: i32 = 520;
        const GRID_START_Y: i32 = 330;
        const GRID_STEP_X: i32 = 170;
        const GRID_STEP_Y: i32 = 205;

        for name in self.active_loadout.iter().take(4) {
            if let Some(config) = self.trap_lookup.get(name) {
                let (tab_x, tab_y) = match config.b_type.as_str() {
                    "Wall" => (172, 375),
                    "Ceiling" => (172, 462),
                    _ => (172, 294),
                };

                if let Ok(mut d) = self.driver.lock() {
                    d.move_to_humanly(tab_x, tab_y, 0.4);
                    d.click_humanly(true, false, 0);
                    thread::sleep(Duration::from_millis(350));

                    let col = config.grid_index[0];
                    let row = config.grid_index[1];
                    let target_x = GRID_START_X + col * GRID_STEP_X;
                    let target_y = GRID_START_Y + row * GRID_STEP_Y;

                    d.move_to_humanly(target_x as u16, target_y as u16, 0.4);
                    d.click_humanly(true, false, 0);
                }
                thread::sleep(Duration::from_millis(400));
            } else {
                println!("⚠️ [Config Error] 未找到陷阱配置: {}", name);
            }
        }
    }

    fn get_absolute_map_pixel(
        &self,
        gx: usize,
        gy: usize,
        w: usize,
        h: usize,
    ) -> Option<(f32, f32)> {
        let meta = self.map_meta.as_ref()?;
        let sx = meta.offset_x + ((gx as f32 + w as f32 / 2.0) * meta.grid_pixel_width);
        let sy = meta.offset_y + ((gy as f32 + h as f32 / 2.0) * meta.grid_pixel_height);
        Some((sx, sy))
    }
    
    fn get_camera_speed(&self, direction: char) -> f32 {
        if let Some(meta) = &self.map_meta {
            match direction {
                'w' => meta.camera_speed_up,
                's' => meta.camera_speed_down,
                'a' => meta.camera_speed_left,
                'd' => meta.camera_speed_right,
                _ => 300.0,
            }
        } else {
            300.0
        }
    }
    
    fn get_camera_bounds(&self) -> (f32, f32, f32, f32) {
        if let Some(meta) = &self.map_meta {
            let max_x = (meta.right - self.config.screen_width).max(0.0);
            let max_y = (meta.bottom - self.config.screen_height).max(0.0);
            (0.0, max_x, 0.0, max_y)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        }
    }
    
    fn clamp_camera_position(&mut self) {
        let (min_x, max_x, min_y, max_y) = self.get_camera_bounds();
        self.camera_offset_x = self.camera_offset_x.clamp(min_x, max_x);
        self.camera_offset_y = self.camera_offset_y.clamp(min_y, max_y);
    }
    
    #[allow(dead_code)]
    fn align_camera_to_corner(&mut self, left: bool, top: bool) {
        let meta = self.map_meta.as_ref().unwrap();
        let max_scroll_x = (meta.right - self.config.screen_width).max(0.0);
        let max_scroll_y = (meta.bottom - self.config.screen_height).max(0.0);

        if let Ok(mut human) = self.driver.lock() {
            let key_y = if top { 'w' } else { 's' };
            let key_x = if left { 'a' } else { 'd' };
            println!("🔄 强制归零: {} {}", if top { "顶部" } else { "底部" }, if left { "左侧" } else { "右侧" });
            
            human.key_hold(key_y, 2500);
            thread::sleep(Duration::from_millis(100));
            human.key_hold(key_x, 2500);
        }
        
        self.camera_offset_x = if left { 0.0 } else { max_scroll_x };
        self.camera_offset_y = if top { 0.0 } else { max_scroll_y };
        self.clamp_camera_position();
        thread::sleep(Duration::from_millis(500));
    }
    
    #[allow(dead_code)]
    fn is_position_in_safe_area(&self, map_x: f32, map_y: f32) -> bool {
        if let Some(meta) = &self.map_meta {
            if meta.viewport_safe_areas.is_empty() {
                return true;
            }
            
            for area in &meta.viewport_safe_areas {
                if map_x >= area.min_x && map_x <= area.max_x &&
                   map_y >= area.min_y && map_y <= area.max_y {
                    return true;
                }
            }
            false
        } else {
            true
        }
    }

    fn get_trap_key(&self, name: &str) -> char {
        let index = self
            .active_loadout
            .iter()
            .position(|t| t == name)
            .unwrap_or(0);
        match index {
            0 => '4',
            1 => '5',
            2 => '6',
            3 => '7',
            _ => '1',
        }
    }

    pub fn run(&mut self, terrain_p: &str, strategy_p: &str, trap_p: &str) {
        self.load_map_terrain(terrain_p);
        self.load_trap_config(trap_p);
        self.load_strategy(strategy_p);

        let mut seen = HashSet::new();
        let mut derived_loadout = Vec::new();

        for b in &self.strategy_buildings {
            if !seen.contains(&b.name) && self.trap_lookup.contains_key(&b.name) {
                seen.insert(b.name.clone());
                derived_loadout.push(b.name.clone());
            }
        }
        for u in &self.strategy_upgrades {
            if !seen.contains(&u.building_name) && self.trap_lookup.contains_key(&u.building_name) {
                seen.insert(u.building_name.clone());
                derived_loadout.push(u.building_name.clone());
            }
        }

        if derived_loadout.is_empty() {
            println!("⚠️ 警告: 策略中未发现已知陷阱，装备栏将为空！");
        } else {
            println!("📋 自动分析策略，生成装备列表: {:?}", derived_loadout);
        }
        self.active_loadout = derived_loadout;

        if let Ok(mut human) = self.driver.lock() {
            println!("👆 点击游戏入口...");
            human.move_to_humanly(1700, 950, 0.5);
            human.click_humanly(true, false, 0);
            human.move_to_humanly(1110, 670, 0.5);
            human.click_humanly(true, false, 0);
        }

        println!("⏳ 等待战斗开始...");
        loop {
            if let Some(status) = self.recognize_wave_status(self.config.hud_check_rect, false) {
                if status.current_wave > 0 {
                    println!("🎮 战斗开始! 初始波次: {}", status.current_wave);
                    self.last_wave_change_time = Instant::now();
                    break;
                }
            }
            thread::sleep(Duration::from_millis(1000));
        }

        self.execute_prep_logic();
        self.setup_view();

        println!("🤖 自动化监控中...");
        let mut no_wave_count = 0;
        let mut skip_count = 0;
        loop {
            // 尝试检测波次 (带 Tab 切换)
            // 我们把结果存下来，以便处理 "未检测到" 的情况
            let wave_status_opt = self.recognize_wave_status(self.config.hud_wave_loop_rect, true);

            if let Some(status) = wave_status_opt {
                // === 情况 A: 正常检测到波次 ===
                no_wave_count = 0; // 重置计数器
                skip_count = 0; // 重置跳过计数
                if self.validate_wave_transition(status.current_wave) {
                    let current_wave = status.current_wave;
                    self.execute_wave_phase(current_wave, false);
                    println!("🔔 波次 {} 前期完成，按 G 开战", current_wave);
                    if let Ok(mut d) = self.driver.lock() {
                        d.key_click('g');
                    }
                    thread::sleep(Duration::from_secs(1));
                    self.execute_wave_phase(current_wave, true);
                }
            } else {
                // === 情况 B: 未检测到波次 (可能是结算界面) ===
                no_wave_count += 1;
                println!(
                    "⚠️ [Monitor] 未检测到波次信息 ({}/2)",
                    no_wave_count
                );

                // 连续2次未检测到波次，才尝试跳过结算
                if no_wave_count >= 2 {
                    skip_count += 1;
                    println!(
                        "   -> 第 {} 次跳过结算 (共需3次)",
                        skip_count
                    );

                    if let Ok(mut d) = self.driver.lock() {
                        println!("   -> 点击空格 (Space) + 双击 ESC");

                        // 直接操作底层设备发送 HID 码 0x29 (ESC)
                        if let Ok(mut dev) = d.device.lock() {
                            // 第一次 ESC
                            dev.key_down(0x29, 0);
                            thread::sleep(Duration::from_millis(100)); // 按下持续时间
                            dev.key_up();

                            thread::sleep(Duration::from_millis(300)); // 两次按键间隔
                        }

                        // 点击空格 (跳过结算动画)
                        d.key_click(' ');
                        thread::sleep(Duration::from_millis(500));

                        if let Ok(mut dev) = d.device.lock() {
                            // 第二次 ESC
                            dev.key_down(0x29, 0);
                            thread::sleep(Duration::from_millis(100));
                            dev.key_up();
                        }
                    }
                }

                // 连续3次跳过结算后，判定为游戏结束
                if skip_count >= 3 {
                    println!("🏁 连续 3 次跳过结算，判定为游戏结束。");
                    println!("🔄 退出当前循环，返回主程序...");
                    break;
                }
            }

            thread::sleep(Duration::from_millis(10000));
        }
    }
}
