// src/main.rs
use clap::{Parser, ValueEnum};
use nzm_cmd::daily_routine::DailyRoutineApp;
use nzm_cmd::hardware::{create_driver, DriverType, InputDriver};
use nzm_cmd::human::HumanDriver;
use nzm_cmd::nav::{NavEngine, NavResult};
use nzm_cmd::tower_defense::TowerDefenseApp;
use screenshots::Screen;
use std::fs;
use std::io::{self, BufRead, Write};
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
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "COM3")]
    port: String,

    #[arg(short, long, default_value = "loop")]
    mode: RunMode,

    #[arg(long)]
    test: Option<String>,
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

fn main() {
    let args = Args::parse();

    println!("========================================");
    println!("🚀 NZM_CMD 智能控制中心");
    println!("📍 端口: {}", args.port);
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

    println!("✅ 引擎就绪，进入交互模式");
    println!("========================================");
    println!("📋 可用命令:");
    println!("   <目标名称>  - 导航并执行目标 (如: 空间站英雄)");
    println!("   daily       - 执行每日任务 (自动检测塔防)");
    println!("   td <地图>   - 执行塔防 (如: td 空间站英雄)");
    println!("   status      - 查看今日执行状态");
    println!("   help        - 显示帮助");
    println!("   exit        - 退出程序");
    println!("========================================");

    let stdin = io::stdin();
    print!("> ");
    io::stdout().flush().unwrap();

    for line in stdin.lock().lines() {
        let input = line.unwrap_or_default().trim().to_string();
        
        if input.is_empty() {
            print!("> ");
            io::stdout().flush().unwrap();
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts.get(0).unwrap_or(&"");

        match *cmd {
            "exit" | "quit" | "q" => {
                println!("� 退出程序");
                break;
            }
            "help" | "h" | "?" => {
                println!("📋 可用命令:");
                println!("   <目标名称>  - 导航并执行目标");
                println!("   daily       - 执行每日任务");
                println!("   td <地图>   - 执行塔防");
                println!("   status      - 查看今日执行状态");
                println!("   exit        - 退出程序");
            }
            "status" => {
                let today = get_today_string();
                println!("📅 今日标记: {}", today);
                
                let scenes = vec!["空间站英雄", "空间站普通", "空间站困难", "空间站炼狱"];
                for scene in scenes {
                    let status = if check_today_executed(scene) {
                        "✅ 已执行"
                    } else {
                        "⬜ 未执行"
                    };
                    println!("   {}: {}", scene, status);
                }
            }
            "daily" => {
                println!("⏳ 1秒后执行...");
                thread::sleep(Duration::from_secs(1));
                execute_daily(Arc::clone(&human_driver), Arc::clone(&engine));
            }
            "td" => {
                if parts.len() > 1 {
                    let target = parts[1..].join(" ");
                    println!("⏳ 1秒后执行...");
                    thread::sleep(Duration::from_secs(1));
                    execute_td(&target, Arc::clone(&human_driver), Arc::clone(&engine));
                } else {
                    println!("❌ 用法: td <地图名称>");
                    println!("   示例: td 空间站英雄");
                }
            }
            _ => {
                println!("⏳ 1秒后执行...");
                thread::sleep(Duration::from_secs(1));
                execute_target(&input, Arc::clone(&human_driver), Arc::clone(&engine));
            }
        }

        print!("> ");
        io::stdout().flush().unwrap();
    }
}

fn execute_daily(driver: Arc<Mutex<HumanDriver>>, engine: Arc<NavEngine>) {
    println!("\n� [Daily] 开始执行每日任务流程...");
    
    let td_scene = "空间站英雄";
    
    if !check_today_executed(td_scene) {
        println!("🎮 [Daily] 今日尚未执行塔防，先执行 [{}]...", td_scene);
        
        println!("🧭 [Daily] 导航至 [{}]...", td_scene);
        let nav_result = engine.navigate(td_scene);
        
        match nav_result {
            NavResult::Handover(scene_id, _) => {
                execute_td_internal(&scene_id, driver.clone(), engine.clone());
                mark_today_executed(&scene_id);
                println!("✅ [Daily] 塔防执行完成");
                
                thread::sleep(Duration::from_secs(3));
            }
            NavResult::Success => {
                println!("✅ [Daily] 已到达目标页面");
            }
            NavResult::Failed => {
                println!("❌ [Daily] 导航失败");
            }
        }
    } else {
        println!("✅ [Daily] 今日已执行过塔防，跳过");
    }

    println!("🧭 [Daily] 导航至 [每日目标]...");
    let nav_result = engine.navigate("每日目标");
    
    match nav_result {
        NavResult::Handover(_, _) => {
            let app = DailyRoutineApp::new(driver, engine);
            app.run();
        }
        NavResult::Success => {
            println!("✅ [Daily] 已到达每日目标页面");
        }
        NavResult::Failed => {
            println!("❌ [Daily] 导航失败");
        }
    }
    
    println!("🏁 [Daily] 每日任务流程结束");
}

fn execute_td(target: &str, driver: Arc<Mutex<HumanDriver>>, engine: Arc<NavEngine>) {
    println!("\n🏰 [TD] 执行塔防: {}", target);
    
    println!("🧭 [TD] 导航至 [{}]...", target);
    let nav_result = engine.navigate(target);
    
    match nav_result {
        NavResult::Handover(scene_id, _) => {
            execute_td_internal(&scene_id, driver, engine);
            println!("✅ [TD] 塔防执行完成");
        }
        NavResult::Success => {
            println!("✅ [TD] 已到达目标页面");
        }
        NavResult::Failed => {
            println!("❌ [TD] 导航失败");
        }
    }
}

fn execute_td_internal(scene_id: &str, driver: Arc<Mutex<HumanDriver>>, engine: Arc<NavEngine>) {
    let mut td_app = TowerDefenseApp::new(driver, engine);

    let map_dir = format!("maps/{}", scene_id);
    let map_file = format!("{}/{}地图.json", map_dir, scene_id);
    let strategy_file = format!("{}/{}策略.json", map_dir, scene_id);
    let traps_file = format!("{}/{}防御塔列表.json", map_dir, scene_id);

    println!("📂 加载塔防配置: {}", map_dir);
    td_app.run(&map_file, &strategy_file, &traps_file);
}

fn execute_target(target: &str, driver: Arc<Mutex<HumanDriver>>, engine: Arc<NavEngine>) {
    println!("\n🎯 执行目标: {}", target);
    
    let nav_result = engine.navigate(target);
    
    match nav_result {
        NavResult::Handover(scene_id, handler_opt) => {
            let handler_key = handler_opt.as_deref().unwrap_or("td");

            match handler_key {
                "daily" => {
                    println!("📅 [路由] 启动日活模块...");
                    let app = DailyRoutineApp::new(driver, engine);
                    app.run();
                }
                "td" | _ => {
                    execute_td_internal(&scene_id, driver, engine);
                }
            }
        }
        NavResult::Success => {
            println!("✅ 已到达目标位置");
        }
        NavResult::Failed => {
            println!("❌ 导航失败");
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

fn run_combo_test(driver: Arc<Mutex<HumanDriver>>) {
    println!("Testing Combo Sequence (Loop)... Press Ctrl+C to stop.");
    let delay = Duration::from_millis(40);

    let key_b = 0x05;
    let key_4 = 0x20;
    let key_5 = 0x21;

    loop {
        if let Ok(mut human) = driver.lock() {
            human.click_humanly(true, false, 50);
            thread::sleep(delay);
            human.click_humanly(true, false, 0);
            thread::sleep(delay);

            if let Ok(mut dev) = human.device.lock() {
                dev.key_down(key_b, 0);
            }
            thread::sleep(delay);
            if let Ok(mut dev) = human.device.lock() {
                dev.key_down(key_5, 0);
            }
            thread::sleep(delay);

            if let Ok(mut dev) = human.device.lock() {
                dev.key_up();
            }
            thread::sleep(delay);
            if let Ok(mut dev) = human.device.lock() {
                dev.key_up();
            }
            
            for _ in 0..20 { thread::sleep(delay); }

            human.click_humanly(true, false, 0);
            thread::sleep(delay);
            human.click_humanly(true, false, 0);
            thread::sleep(delay);

            if let Ok(mut dev) = human.device.lock() {
                dev.key_down(key_b, 0);
            }
            thread::sleep(delay);
            if let Ok(mut dev) = human.device.lock() {
                dev.key_down(key_4, 0);
            }
            thread::sleep(delay);

            if let Ok(mut dev) = human.device.lock() {
                dev.key_up();
            }
            thread::sleep(delay);
            if let Ok(mut dev) = human.device.lock() {
                dev.key_up();
            }
            
            for _ in 0..25 { thread::sleep(delay); }
        }
    }
}
