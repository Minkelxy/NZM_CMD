// src/main.rs
use clap::{Parser, ValueEnum};
use nzm_cmd::daily_routine::DailyRoutineApp;
use nzm_cmd::hardware::{create_driver, DriverType, InputDriver};
use nzm_cmd::human::HumanDriver;
use nzm_cmd::nav::{NavEngine, NavResult};
use nzm_cmd::tower_defense::TowerDefenseApp;
use screenshots::Screen;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(ValueEnum, Clone, Debug, Default, PartialEq)]
enum RunMode {
    #[default]
    Loop,
    Once,
    Daily,
    Schedule,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "COM3")]
    port: String,

    #[arg(short, long, default_value = "空间站普通")]
    target: String,

    #[arg(long)]
    test: Option<String>,

    #[arg(short, long, default_value = "loop")]
    mode: RunMode,

    #[arg(short, long)]
    schedule: Option<String>,
}

const TD_RECORD_FILE: &str = "td_record.txt";

fn get_today_string() -> String {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let days = duration.as_secs() / 86400;
    format!("{}", days)
}

fn check_today_executed(scene_id: &str) -> bool {
    let file = format!("{}_{}", TD_RECORD_FILE, scene_id);
    if !Path::new(&file).exists() {
        return false;
    }
    if let Ok(content) = fs::read_to_string(&file) {
        let today = get_today_string();
        return content.trim() == today;
    }
    false
}

fn mark_today_executed(scene_id: &str) {
    let file = format!("{}_{}", TD_RECORD_FILE, scene_id);
    let today = get_today_string();
    let _ = fs::write(&file, &today);
}

fn get_current_time() -> (u32, u32) {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let total_secs = duration.as_secs();
    let hours = ((total_secs % 86400) / 3600 + 8) % 24;
    let minutes = (total_secs % 3600) / 60;
    (hours as u32, minutes as u32)
}

fn parse_schedule_time(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let hours: u32 = parts[0].parse().ok()?;
    let minutes: u32 = parts[1].parse().ok()?;
    if hours < 24 && minutes < 60 {
        Some((hours, minutes))
    } else {
        None
    }
}

fn wait_until_schedule(target_time: (u32, u32)) {
    loop {
        let current = get_current_time();
        if current.0 == target_time.0 && current.1 == target_time.1 {
            println!("⏰ 到达预定时间，开始执行！");
            return;
        }
        
        let current_mins = current.0 * 60 + current.1;
        let target_mins = target_time.0 * 60 + target_time.1;
        let diff_mins = if target_mins > current_mins {
            target_mins - current_mins
        } else {
            1440 - current_mins + target_mins
        };
        
        println!("⏰ 当前时间 {:02}:{:02}，等待至 {:02}:{:02} (还需 {} 分钟)", 
            current.0, current.1, target_time.0, target_time.1, diff_mins);
        
        thread::sleep(Duration::from_secs(60));
    }
}

fn main() {
    let args = Args::parse();

    println!("========================================");
    println!("🚀 NZM_CMD 智能控制中心");
    println!("📍 端口: {}", args.port);
    if let Some(t) = &args.test {
        println!("🔧 模式: 测试 ({})", t);
    } else {
        println!("🎯 目标: {}", args.target);
    }
    println!("========================================");

    let (sw, sh) = (1920, 1080);

    let driver_type = if args.port.to_uppercase() == "SOFT" {
        DriverType::Software
    } else {
        DriverType::Hardware
    };

    let driver_box: Box<dyn InputDriver> = match create_driver(driver_type, &args.port, sw, sh) {
        Ok(d) => d,
        Err(e) => {
            println!("⚠️ 警告: 无法初始化驱动 ({})", e);
            println!("⚠️ 尝试回退到 [软件模拟模式]...");
            create_driver(DriverType::Software, "", sw, sh).unwrap()
        }
    };

    let driver_arc: Arc<Mutex<Box<dyn InputDriver>>> = Arc::new(Mutex::new(driver_box));

    let hb = Arc::clone(&driver_arc);
    thread::spawn(move || loop {
        if let Ok(mut d) = hb.lock() {
            d.heartbeat();
        }
        thread::sleep(Duration::from_secs(1));
    });

    let human_driver = Arc::new(Mutex::new(HumanDriver::new(
        Arc::clone(&driver_arc),
        sw / 2,
        sh / 2,
    )));

    let engine = Arc::new(NavEngine::new("ui_map.toml", Arc::clone(&human_driver)));

    if let Some(mode) = args.test.as_deref() {
        println!("⏳ 5秒后开始执行 [{}] 测试...", mode);
        thread::sleep(Duration::from_secs(5));
        match mode {
            "input" => run_input_test(human_driver),
            "screen" => run_screen_test(),
            "ocr" => run_ocr_test(engine),
            "scroll" => run_scroll_test(human_driver),
            "combo" => run_combo_test(human_driver),
            _ => println!("❌ 未知测试模式"),
        }
        return;
    }

    println!("✅ 引擎就绪，5秒后开始自动化...");
    println!("📋 执行模式: {:?}", args.mode);
    if let Some(ref t) = args.schedule {
        println!("⏰ 定时: {}", t);
    }
    thread::sleep(Duration::from_secs(5));

    if args.mode == RunMode::Schedule {
        if let Some(schedule_time) = &args.schedule {
            if let Some(target_time) = parse_schedule_time(schedule_time) {
                wait_until_schedule(target_time);
            } else {
                println!("❌ 定时格式错误，请使用 HH:MM 格式 (如 03:00)");
                return;
            }
        } else {
            println!("❌ 定时模式需要指定 --schedule 参数");
            return;
        }
    }

    let should_loop = args.mode == RunMode::Loop;

    loop {
        println!("\n🔄 [主控] 正在导航至: {}...", args.target);

        let nav_result = engine.navigate(&args.target);

        match nav_result {
            NavResult::Handover(scene_id, handler_opt) => {
                println!("⚔️ [主控] 导航成功: [{}]", scene_id);

                let handler_key = handler_opt.as_deref().unwrap_or("td");

                match handler_key {
                    "daily" => {
                        println!("📅 [路由] 检测到 'daily' 标记，启动日活模块...");
                        let app =
                            DailyRoutineApp::new(Arc::clone(&human_driver), Arc::clone(&engine));
                        app.run();
                    }
                    "td" | _ => {
                        let execute = match args.mode {
                            RunMode::Daily => {
                                if check_today_executed(&scene_id) {
                                    println!("✅ [Daily] 今日已执行过 [{}]，跳过。", scene_id);
                                    false
                                } else {
                                    println!("🎮 [Daily] 今日尚未执行，开始执行 [{}]...", scene_id);
                                    true
                                }
                            }
                            RunMode::Loop | RunMode::Once | RunMode::Schedule => true,
                        };

                        if execute {
                            println!("🏰 [路由] 启动塔防模块 (Handler: {})...", handler_key);
                            let mut td_app =
                                TowerDefenseApp::new(Arc::clone(&human_driver), Arc::clone(&engine));

                            let map_dir = format!("maps/{}", scene_id);
                            let map_file = format!("{}/{}地图.json", map_dir, scene_id);
                            let strategy_file = format!("{}/{}策略.json", map_dir, scene_id);
                            let traps_file = format!("{}/{}防御塔列表.json", map_dir, scene_id);

                            println!("📂 加载配置目录: {}", map_dir);
                            println!("   📍 地图: {}", map_file);
                            println!("   📍 策略: {}", strategy_file);
                            println!("   📍 防御塔: {}", traps_file);
                            td_app.run(&map_file, &strategy_file, &traps_file);

                            if args.mode == RunMode::Daily {
                                mark_today_executed(&scene_id);
                                println!("✅ [Daily] 已标记今日执行完成。");
                            }
                        }
                    }
                }

                if !should_loop {
                    println!("🎉 单次执行完成，退出程序。");
                    break;
                }
                println!("🎉 本局任务结束，5秒后重新开始循环...");
                thread::sleep(Duration::from_secs(5));
            }

            NavResult::Failed => {
                println!("❌ [主控] 导航失败，执行重置操作 (ESC)...");

                if let Ok(mut human) = human_driver.lock() {
                    human.key_hold('\u{1B}', 100);

                    if let Ok(mut dev) = human.device.lock() {
                        dev.key_down(0x29, 0);
                    }
                    thread::sleep(Duration::from_millis(100));
                    if let Ok(mut dev) = human.device.lock() {
                        dev.key_up();
                    }

                    thread::sleep(Duration::from_millis(100));
                    if let Ok(mut dev) = human.device.lock() {
                        dev.key_down(0x2C, 0);
                    }
                    thread::sleep(Duration::from_millis(100));
                    if let Ok(mut dev) = human.device.lock() {
                        dev.key_up(); 
                    }
                }

                println!("⏳ 等待界面重置 (3秒)...");
                thread::sleep(Duration::from_secs(3));
            }

            NavResult::Success => {
                println!("✅ [主控] 导航到达终点，等待重置...");
                thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

fn run_input_test(driver: Arc<Mutex<HumanDriver>>) {
    println!("Testing Mouse & Keyboard...");
    if let Ok(mut d) = driver.lock() {
        println!("-> 移动鼠标 (矩形轨迹)");
        let start_x = 500;
        let start_y = 500;
        d.move_to_humanly(start_x, start_y, 0.5);
        d.move_to_humanly(start_x + 300, start_y, 0.5);
        d.move_to_humanly(start_x + 300, start_y + 300, 0.5);
        d.move_to_humanly(start_x, start_y + 300, 0.5);
        d.move_to_humanly(start_x, start_y, 0.5);

        println!("-> 执行点击 (Click)");
        d.click_humanly(true, false, 0);
        thread::sleep(Duration::from_millis(500));

        println!("-> 模拟键盘输入 'hello 123'");
        d.type_humanly("hello 123", 60.0);
    }
    println!("Done.");
}

fn run_screen_test() {
    println!("Testing Screen Capture...");
    let start = Instant::now();
    let screens = Screen::all().unwrap_or_default();

    if let Some(screen) = screens.first() {
        println!(
            "-> 检测到屏幕: {}x{}",
            screen.display_info.width, screen.display_info.height
        );
        match screen.capture() {
            Ok(image) => {
                let path = "debug_screenshot.png";
                image.save(path).unwrap();
                println!(
                    "✅ 截图成功! 已保存至: {} (耗时 {}ms)",
                    path,
                    start.elapsed().as_millis()
                );
            }
            Err(e) => println!("❌ 截图失败: {}", e),
        }
    } else {
        println!("❌ 未检测到显示器");
    }
}

fn run_ocr_test(engine: Arc<NavEngine>) {
    println!("Testing OCR Function...");
    let rect = [100, 100, 500, 200];
    println!("-> 正在识别区域: {:?}", rect);
    let start = Instant::now();
    let text = engine.ocr_area(rect);

    println!("----------------------------------------");
    println!("⏱️ 耗时: {} ms", start.elapsed().as_millis());
    println!("📝 识别结果: [{}]", text);
    println!("----------------------------------------");

    if text.is_empty() {
        println!("⚠️ 警告: 识别结果为空，请确认该区域有文字。");
    }
}

fn run_scroll_test(driver: Arc<Mutex<HumanDriver>>) {
    println!("Testing Mouse Scroll...");
    if let Ok(mut d) = driver.lock() {
        println!("-> 向下滚动 5 格 (Scroll Down)");
        d.mouse_scroll(-5);

        thread::sleep(Duration::from_secs(2));

        println!("-> 向上滚动 5 格 (Scroll Up)");
        d.mouse_scroll(5);
    }
    println!("Done.");
}

// ✨ 新增 Combo 测试函数
fn run_combo_test(driver: Arc<Mutex<HumanDriver>>) {
    println!("Testing Combo Sequence (Loop)... Press Ctrl+C to stop.");
    // 默认间隔 50ms
    let delay = Duration::from_millis(40);

    // HID 键码: b=0x05, 4=0x21, 5=0x22
    let key_b = 0x05;
    let key_4 = 0x20;
    let key_5 = 0x21;

    loop {
        // 锁定 HumanDriver 以获取访问权限
        if let Ok(mut human) = driver.lock() {
            // 1. 鼠标左键两下
            // (click_humanly 内部会有几十毫秒的 hold time)
            human.click_humanly(true, false, 50);
            thread::sleep(delay);
            human.click_humanly(true, false, 0);
            thread::sleep(delay);

            // 2. 按 b, 按 5
            if let Ok(mut dev) = human.device.lock() {
                dev.key_down(key_b, 0);
            }
            thread::sleep(delay);
            if let Ok(mut dev) = human.device.lock() {
                dev.key_down(key_5, 0);
            }
            thread::sleep(delay);

            // 3. 松 b, 松 5
            if let Ok(mut dev) = human.device.lock() {
                dev.key_up(); // 释放 (通常是释放所有或最后一个)
            }
            thread::sleep(delay);
            if let Ok(mut dev) = human.device.lock() {
                dev.key_up(); // 再次释放以防万一
            }
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            // 4. 鼠标左键两下
            human.click_humanly(true, false, 0);
            thread::sleep(delay);
            human.click_humanly(true, false, 0);
            thread::sleep(delay);

            // 5. 按 b, 按 4
            if let Ok(mut dev) = human.device.lock() {
                dev.key_down(key_b, 0);
            }
            thread::sleep(delay);
            if let Ok(mut dev) = human.device.lock() {
                dev.key_down(key_4, 0);
            }
            thread::sleep(delay);

            // 6. 松 b, 松 4
            if let Ok(mut dev) = human.device.lock() {
                dev.key_up();
            }
            thread::sleep(delay);
            if let Ok(mut dev) = human.device.lock() {
                dev.key_up();
            }
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
            thread::sleep(delay);
        }
        // 循环继续
    }
}
