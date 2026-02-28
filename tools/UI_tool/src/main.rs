#![windows_subsystem = "windows"]

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use screenshots::Screen;
use serde::Deserialize;
use std::fs;
use std::time::Instant;
use std::collections::VecDeque;

// OCR 所需的引用
use std::io::Cursor;
use windows::Media::Ocr::{OcrEngine, OcrResult}; 
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

// ==========================================
// 1. 数据结构
// ==========================================
#[derive(Clone, PartialEq)]
enum RecognitionLogic { AND, OR }

#[derive(Clone, PartialEq)]
enum ElementKind {
    TextAnchor { text: String },
    ColorAnchor { color_hex: String, tolerance: u8 },
    Button { target: String, post_delay: u32 },
}

#[derive(Clone)]
struct UIElementDraft {
    pos_or_rect: Rect,
    kind: ElementKind,
}

#[derive(Deserialize)]
struct TomlRoot { scenes: Vec<TomlScene> }
#[derive(Deserialize)]
struct TomlScene { id: String, name: String, logic: Option<String>, anchors: Option<TomlAnchors>, transitions: Option<Vec<TomlTransition>>, handler: Option<String> }
#[derive(Deserialize)]
struct TomlAnchors { text: Option<Vec<TomlTextAnchor>>, color: Option<Vec<TomlColorAnchor>> }
#[derive(Deserialize)]
struct TomlTextAnchor { rect: [i32; 4], val: String }
#[derive(Deserialize)]
struct TomlColorAnchor { pos: [i32; 2], val: String, tol: u8 }
#[derive(Deserialize)]
struct TomlTransition { target: String, coords: [i32; 2], post_delay: u32 }

// ==========================================
// 1.5 场景结构
// ==========================================
#[derive(Clone)]
struct Scene {
    id: String,
    name: String,
    logic: RecognitionLogic,
    drafts: Vec<UIElementDraft>,
    handler: Option<String>,
    viz_pos: Pos2,
    viz_size: Vec2,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            logic: RecognitionLogic::AND,
            drafts: Vec::new(),
            handler: None,
            viz_pos: Pos2::ZERO,
            viz_size: Vec2::new(150.0, 80.0),
        }
    }
}

// ==========================================
// 2. 编辑器状态
// ==========================================
struct MapBuilderTool {
    texture: Option<egui::TextureHandle>,
    raw_image: Option<image::RgbaImage>, 
    img_size: Vec2,
    
    ocr_engine: Option<OcrEngine>,
    ocr_test_result: String, 

    scenes: Vec<Scene>,
    current_scene_index: usize,
    
    start_pos: Option<Pos2>,
    current_rect: Option<Rect>,
    is_color_picker_mode: bool,
    capture_timer: Option<Instant>, 

    toml_content: String,
    status_msg: String,
    
    // 可视化相关
    show_visualization: bool,
    viz_dragging_scene: Option<usize>,
    viz_drag_offset: Vec2,
    viz_pan: Vec2,
    viz_zoom: f32,
}

impl MapBuilderTool {
    fn current_scene(&self) -> &Scene {
        &self.scenes[self.current_scene_index]
    }
    
    fn current_scene_mut(&mut self) -> &mut Scene {
        &mut self.scenes[self.current_scene_index]
    }
    
    fn add_new_scene(&mut self) {
        let new_id = format!("scene_{}", self.scenes.len() + 1);
        let new_name = format!("新场景 {}", self.scenes.len() + 1);
        let viz_pos = Pos2::new(
            100.0 + (self.scenes.len() as f32 * 200.0) % 800.0,
            100.0 + (self.scenes.len() as f32 * 150.0) % 600.0
        );
        self.scenes.push(Scene {
            id: new_id,
            name: new_name,
            logic: RecognitionLogic::AND,
            drafts: Vec::new(),
            handler: None,
            viz_pos,
            viz_size: Vec2::new(150.0, 80.0),
        });
        self.current_scene_index = self.scenes.len() - 1;
        self.status_msg = "已添加新场景".into();
    }
    
    fn delete_current_scene(&mut self) {
        if self.scenes.len() > 1 {
            self.scenes.remove(self.current_scene_index);
            if self.current_scene_index >= self.scenes.len() {
                self.current_scene_index = self.scenes.len() - 1;
            }
            self.status_msg = "已删除场景".into();
        } else {
            self.status_msg = "⚠️ 至少需要保留一个场景".into();
        }
    }
    
    fn duplicate_current_scene(&mut self) {
        let scene = self.current_scene().clone();
        let new_id = format!("{}_{}", scene.id, self.scenes.len() + 1);
        let new_name = format!("{} 副本", scene.name);
        let new_viz_pos = Pos2::new(scene.viz_pos.x + 50.0, scene.viz_pos.y + 50.0);
        self.scenes.push(Scene {
            id: new_id,
            name: new_name,
            logic: scene.logic,
            drafts: scene.drafts.clone(),
            handler: scene.handler.clone(),
            viz_pos: new_viz_pos,
            viz_size: scene.viz_size,
        });
        self.current_scene_index = self.scenes.len() - 1;
        self.status_msg = "已复制场景".into();
    }
}

unsafe impl Send for MapBuilderTool {}

impl MapBuilderTool {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        
        let engine = OcrEngine::TryCreateFromUserProfileLanguages().ok();
        let status = if engine.is_some() { "OCR 引擎就绪" } else { "⚠️ OCR 初始化失败" };

        let initial_scene = Scene {
            id: "lobby_01".into(),
            name: "游戏主界面".into(),
            logic: RecognitionLogic::AND,
            drafts: Vec::new(),
            handler: None,
            viz_pos: Pos2::new(100.0, 100.0),
            viz_size: Vec2::new(150.0, 80.0),
        };

        Self {
            texture: None,
            raw_image: None,
            img_size: Vec2::ZERO,
            ocr_engine: engine,          
            ocr_test_result: String::new(), 
            scenes: vec![initial_scene],
            current_scene_index: 0,
            start_pos: None,
            current_rect: None,
            is_color_picker_mode: false,
            capture_timer: None,
            toml_content: String::new(),
            status_msg: status.into(),
            
            show_visualization: false,
            viz_dragging_scene: None,
            viz_drag_offset: Vec2::ZERO,
            viz_pan: Vec2::ZERO,
            viz_zoom: 1.0,
        }
    }

    fn capture_immediate(&mut self, ctx: &egui::Context) {
        let screens = Screen::all().unwrap();
        if let Some(screen) = screens.first() {
            if let Ok(image) = screen.capture() {
                self.img_size = Vec2::new(image.width() as f32, image.height() as f32);
                self.raw_image = Some(image.clone()); 
                let color_img = egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize], 
                    image.as_flat_samples().as_slice()
                );
                self.texture = Some(ctx.load_texture("shot", color_img, Default::default()));
                self.status_msg = "截图成功".into();
            }
        }
    }

    fn pick_color(&self, p: Pos2) -> String {
        if let Some(img) = &self.raw_image {
            let x = p.x as u32;
            let y = p.y as u32;
            if x < img.width() && y < img.height() {
                let pixel = img.get_pixel(x, y);
                return format!("#{:02X}{:02X}{:02X}", pixel[0], pixel[1], pixel[2]);
            }
        }
        "#FFFFFF".into()
    }

    fn build_toml(&mut self) {
        let mut toml = String::new();
        
        for scene in &self.scenes {
            let logic_str = if scene.logic == RecognitionLogic::AND { "and" } else { "or" };
            toml.push_str(&format!("[[scenes]]\nid = \"{}\"\nname = \"{}\"\nlogic = \"{}\"\n", scene.id, scene.name, logic_str));
            
            if let Some(handler) = &scene.handler {
                toml.push_str(&format!("handler = \"{}\"\n", handler));
            }
            
            toml.push_str("\n[scenes.anchors]\n");
            toml.push_str("text = [\n");
            
            for d in scene.drafts.iter() {
                if let ElementKind::TextAnchor { text } = &d.kind {
                    toml.push_str(&format!("  {{ rect = [{}, {}, {}, {}], val = \"{}\" }},\n",
                        d.pos_or_rect.min.x as i32, d.pos_or_rect.min.y as i32, d.pos_or_rect.max.x as i32, d.pos_or_rect.max.y as i32, text));
                }
            }
            
            toml.push_str("]\ncolor = [\n");
            
            for d in scene.drafts.iter() {
                if let ElementKind::ColorAnchor { color_hex, tolerance } = &d.kind {
                    toml.push_str(&format!("  {{ pos = [{}, {}], val = \"{}\" , tol = {} }},\n",
                        d.pos_or_rect.min.x as i32, d.pos_or_rect.min.y as i32, color_hex, tolerance));
                }
            }
            
            toml.push_str("]\n\n# --- 动作步骤 ---\n");
            
            for d in scene.drafts.iter() {
                if let ElementKind::Button { target, post_delay } = &d.kind {
                    toml.push_str("[[scenes.transitions]]\n");
                    toml.push_str(&format!("target = \"{}\"\n", target));
                    toml.push_str(&format!("coords = [{}, {}]\n", d.pos_or_rect.center().x as i32, d.pos_or_rect.center().y as i32));
                    toml.push_str(&format!("post_delay = {}\n\n", post_delay));
                }
            }
            
            toml.push_str("\n");
        }
        
        self.toml_content = toml;
        self.status_msg = "TOML 已生成".into();
    }

    fn import_toml(&mut self) {
        if self.toml_content.trim().is_empty() { self.status_msg = "导入失败：内容为空".into(); return; }
        match toml::from_str::<TomlRoot>(&self.toml_content) {
            Ok(root) => {
                self.scenes.clear();
                
                let mut temp_scenes: Vec<(usize, String, String, Option<String>, Vec<UIElementDraft>, Option<String>)> = Vec::new();
                
                for (idx, scene) in root.scenes.iter().enumerate() {
                    let mut drafts = Vec::new();
                    
                    if let Some(anchors) = &scene.anchors {
                        if let Some(texts) = &anchors.text {
                            for t in texts {
                                let rect = Rect::from_min_max(Pos2::new(t.rect[0] as f32, t.rect[1] as f32), Pos2::new(t.rect[2] as f32, t.rect[3] as f32));
                                drafts.push(UIElementDraft { pos_or_rect: rect, kind: ElementKind::TextAnchor { text: t.val.clone() } });
                            }
                        }
                        if let Some(colors) = &anchors.color {
                            for c in colors {
                                let pos = Pos2::new(c.pos[0] as f32, c.pos[1] as f32);
                                let rect = Rect::from_min_max(pos, pos + Vec2::splat(1.0));
                                drafts.push(UIElementDraft { pos_or_rect: rect, kind: ElementKind::ColorAnchor { color_hex: c.val.clone(), tolerance: c.tol } });
                            }
                        }
                    }
                    if let Some(transitions) = &scene.transitions {
                        for t in transitions {
                            let rect = Rect::from_center_size(Pos2::new(t.coords[0] as f32, t.coords[1] as f32), Vec2::splat(20.0));
                            drafts.push(UIElementDraft { pos_or_rect: rect, kind: ElementKind::Button { target: t.target.clone(), post_delay: t.post_delay } });
                        }
                    }
                    
                    let handler = scene.handler.clone();
                    
                    let logic = match scene.logic {
                        Some(ref logic_str) => match logic_str.to_lowercase().as_str() {
                            "or" => RecognitionLogic::OR,
                            "and" => RecognitionLogic::AND,
                            _ => {
                                eprintln!("Warning: Unknown logic value '{}', defaulting to AND", logic_str);
                                RecognitionLogic::AND
                            }
                        },
                        None => RecognitionLogic::AND,
                    };
                    
                    temp_scenes.push((idx, scene.id.clone(), scene.name.clone(), Some(if logic == RecognitionLogic::AND { "and" } else { "or" }.to_string()), drafts, handler));
                }
                
                let positions = self.calculate_layout(&root.scenes);
                
                for (idx, id, name, logic, drafts, handler) in temp_scenes {
                    let logic_val = if let Some(ref logic_str) = logic {
                        if logic_str == "or" { RecognitionLogic::OR } else { RecognitionLogic::AND }
                    } else {
                        RecognitionLogic::AND
                    };
                    
                    self.scenes.push(Scene {
                        id,
                        name,
                        logic: logic_val,
                        drafts,
                        handler,
                        viz_pos: positions.get(&idx).copied().unwrap_or(Pos2::new(100.0, 100.0)),
                        viz_size: Vec2::new(150.0, 80.0),
                    });
                }
                
                if !self.scenes.is_empty() {
                    self.current_scene_index = 0;
                    self.status_msg = format!("成功导入 {} 个场景", self.scenes.len());
                } else {
                    self.status_msg = "导入失败：未找到场景".into();
                }
            },
            Err(e) => { self.status_msg = format!("解析失败: {}", e); }
        }
    }
    
    fn calculate_layout(&self, scenes: &[TomlScene]) -> std::collections::HashMap<usize, Pos2> {
        use std::collections::{HashMap, HashSet};
        
        let mut positions = HashMap::new();
        let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut parents: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut scene_ids: HashMap<String, usize> = HashMap::new();
        
        for (idx, scene) in scenes.iter().enumerate() {
            scene_ids.insert(scene.id.clone(), idx);
            children.insert(idx, Vec::new());
            parents.insert(idx, Vec::new());
        }
        
        for (idx, scene) in scenes.iter().enumerate() {
            if let Some(transitions) = &scene.transitions {
                for t in transitions {
                    if let Some(&target_idx) = scene_ids.get(&t.target) {
                        children.entry(idx).or_insert_with(Vec::new).push(target_idx);
                        parents.entry(target_idx).or_insert_with(Vec::new).push(idx);
                    }
                }
            }
        }
        
        let mut visited = HashSet::new();
        let mut levels: HashMap<usize, usize> = HashMap::new();
        
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        
        for (idx, parent_list) in &parents {
            if parent_list.is_empty() {
                queue.push_back((*idx, 0));
                levels.insert(*idx, 0);
            }
        }
        
        if queue.is_empty() && !scenes.is_empty() {
            queue.push_back((0, 0));
            levels.insert(0, 0);
        }
        
        while let Some((idx, level)) = queue.pop_front() {
            if visited.contains(&idx) {
                continue;
            }
            visited.insert(idx);
            
            if let Some(child_list) = children.get(&idx) {
                for &child in child_list {
                    let new_level = level + 1;
                    let current_level = levels.get(&child).copied().unwrap_or(usize::MAX);
                    if new_level < current_level {
                        levels.insert(child, new_level);
                    }
                    if !visited.contains(&child) {
                        queue.push_back((child, new_level));
                    }
                }
            }
        }
        
        let mut level_groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for (idx, level) in &levels {
            level_groups.entry(*level).or_insert_with(Vec::new).push(*idx);
        }
        
        let scene_width = 180.0;
        let scene_height = 100.0;
        let horizontal_gap = 50.0;
        let vertical_gap = 80.0;
        
        let start_x = 100.0;
        let start_y = 100.0;
        
        for level in 0..=levels.values().copied().max().unwrap_or(0) {
            if let Some(scenes_at_level) = level_groups.get(&level) {
                let current_y = start_y + level as f32 * (scene_height + vertical_gap);
                
                for (i, &idx) in scenes_at_level.iter().enumerate() {
                    let current_x = start_x + i as f32 * (scene_width + horizontal_gap);
                    positions.insert(idx, Pos2::new(current_x, current_y));
                }
            }
        }
        
        positions
    }

    fn perform_ocr(&mut self, rect: Rect) {
        if self.ocr_engine.is_none() {
            self.ocr_test_result = "OCR 引擎未初始化".into();
            return;
        }
        if let Some(img) = &self.raw_image {
            let x = rect.min.x.max(0.0) as u32;
            let y = rect.min.y.max(0.0) as u32;
            let w = rect.width().max(1.0) as u32;
            let h = rect.height().max(1.0) as u32;

            if x + w > img.width() || y + h > img.height() {
                self.ocr_test_result = "区域超出图片范围".into();
                return;
            }

            let sub_img = image::imageops::crop_imm(img, x, y, w, h).to_image();
            let scaled_img = image::imageops::resize(&sub_img, w * 2, h * 2, image::imageops::FilterType::Lanczos3);
            
            self.ocr_test_result = "识别中...".into();
            let engine = self.ocr_engine.as_ref().unwrap();
            
            let mut results: Vec<String> = Vec::new();
            
            let thresholds: Vec<u8> = vec![100, 120, 140, 160, 180];
            for thresh in &thresholds {
                let mut luma = image::imageops::grayscale(&scaled_img);
                for pixel in luma.pixels_mut() { pixel[0] = if pixel[0] > *thresh { 255 } else { 0 }; }
                let dynamic_img = image::DynamicImage::ImageLuma8(luma);
                
                if let Some(txt) = self.run_ocr_on_image(engine, &dynamic_img) {
                    if !txt.is_empty() { results.push(txt); }
                }
            }

            let original_dynamic = image::DynamicImage::ImageRgba8(scaled_img);
            if let Some(txt) = self.run_ocr_on_image(engine, &original_dynamic) {
                if !txt.is_empty() { results.push(txt); }
            }

            let final_result = self.fuse_ocr_results(&results);
            self.ocr_test_result = if final_result.is_empty() { "无文字".to_string() } else { final_result };
            self.status_msg = format!("OCR 完成: {}", self.ocr_test_result);
        }
    }

    fn run_ocr_on_image(&self, engine: &OcrEngine, dynamic_img: &image::DynamicImage) -> Option<String> {
        let mut png_buffer = Cursor::new(Vec::new());
        dynamic_img.write_to(&mut png_buffer, image::ImageFormat::Png).ok()?;
        let png_bytes = png_buffer.into_inner();

        let stream = InMemoryRandomAccessStream::new().ok()?;
        let writer = DataWriter::CreateDataWriter(&stream).ok()?;
        writer.WriteBytes(&png_bytes).ok()?;
        writer.StoreAsync().ok()?.get().ok()?;
        writer.FlushAsync().ok()?.get().ok()?;
        stream.Seek(0).ok()?;

        let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.get().ok()?;
        let bmp = decoder.GetSoftwareBitmapAsync().ok()?.get().ok()?;
        let result: OcrResult = engine.RecognizeAsync(&bmp).ok()?.get().ok()?;

        let mut text = String::new();
        if let Ok(lines) = result.Lines() {
            for line in lines {
                if let Ok(h_str) = line.Text() {
                    text.push_str(&h_str.to_string());
                }
            }
        }
        Some(text.replace(char::is_whitespace, ""))
    }

    fn fuse_ocr_results(&self, results: &[String]) -> String {
        if results.is_empty() { return String::new(); }
        if results.len() == 1 { return results[0].clone(); }

        use std::collections::HashMap;
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for r in results {
            *counts.entry(r.as_str()).or_insert(0) += 1;
        }

        let mut best: (&str, usize) = ("", 0);
        for (text, count) in &counts {
            if *count > best.1 || (*count == best.1 && text.len() > best.0.len()) {
                best = (text, *count);
            }
        }
        best.0.to_string()
    }
    
    fn draw_visualization_panel(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::drag());
        let rect = resp.rect;
        
        // 绘制背景网格
        self.draw_grid(&painter, rect);
        
        // 应用平移和缩放
        let transform = |p: Pos2| Pos2::new(
            p.x * self.viz_zoom + self.viz_pan.x + rect.min.x,
            p.y * self.viz_zoom + self.viz_pan.y + rect.min.y
        );
        let inverse_transform = |p: Pos2| Pos2::new(
            (p.x - rect.min.x - self.viz_pan.x) / self.viz_zoom,
            (p.y - rect.min.y - self.viz_pan.y) / self.viz_zoom
        );
        
        // 绘制场景连接线
        self.draw_scene_connections(&painter, &transform);
        
        // 绘制场景矩形
        let mut clicked_scene = None;
        for (i, scene) in self.scenes.iter().enumerate() {
            let scene_rect = Rect::from_min_size(transform(scene.viz_pos), scene.viz_size * self.viz_zoom);
            let is_selected = i == self.current_scene_index;
            let has_handler = scene.handler.is_some();
            
            // 场景背景色
            let bg_color = if is_selected {
                Color32::from_rgb(100, 150, 255)
            } else if has_handler {
                Color32::from_rgb(150, 200, 150)
            } else {
                Color32::from_rgb(200, 200, 220)
            };
            
            painter.rect_filled(scene_rect, 0.0, bg_color);
            painter.rect_stroke(scene_rect, 0.0, Stroke::new(2.0, Color32::BLACK));
            
            // 场景名称
            let text_pos = scene_rect.min + Vec2::new(5.0, 5.0);
            painter.text(
                text_pos,
                egui::Align2::LEFT_TOP,
                &scene.name,
                egui::FontId::default(),
                Color32::BLACK
            );
            
            // 场景ID
            let id_pos = scene_rect.min + Vec2::new(5.0, 20.0);
            painter.text(
                id_pos,
                egui::Align2::LEFT_TOP,
                &format!("ID: {}", scene.id),
                egui::FontId::proportional(10.0),
                Color32::from_rgb(80, 80, 80)
            );
            
            // Handler信息
            if let Some(handler) = &scene.handler {
                let handler_pos = scene_rect.min + Vec2::new(5.0, 35.0);
                painter.text(
                    handler_pos,
                    egui::Align2::LEFT_TOP,
                    &format!("Handler: {}", handler),
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(0, 100, 0)
                );
            }
            
            // 检测点击
            if resp.clicked() && scene_rect.contains(resp.hover_pos().unwrap_or(Pos2::ZERO)) {
                clicked_scene = Some(i);
            }
        }
        
        // 处理场景选择
        if let Some(scene_idx) = clicked_scene {
            self.current_scene_index = scene_idx;
            self.status_msg = format!("已选择场景：{}", self.scenes[scene_idx].name);
        }
        
        // 处理拖拽
        if resp.drag_started() {
            for (i, scene) in self.scenes.iter().enumerate() {
                let scene_rect = Rect::from_min_size(transform(scene.viz_pos), scene.viz_size * self.viz_zoom);
                if let Some(mouse_pos) = resp.hover_pos() {
                    if scene_rect.contains(mouse_pos) {
                        self.viz_dragging_scene = Some(i);
                        let inv_pos = inverse_transform(mouse_pos);
                        self.viz_drag_offset = Vec2::new(
                            scene.viz_pos.x - inv_pos.x,
                            scene.viz_pos.y - inv_pos.y
                        );
                        break;
                    }
                }
            }
        }
        
        if let Some(dragging_idx) = self.viz_dragging_scene {
            if let Some(mouse_pos) = resp.interact_pointer_pos() {
                let inv_pos = inverse_transform(mouse_pos);
                self.scenes[dragging_idx].viz_pos = Pos2::new(
                    inv_pos.x + self.viz_drag_offset.x,
                    inv_pos.y + self.viz_drag_offset.y
                );
            }
            if resp.drag_released() {
                self.viz_dragging_scene = None;
            }
        }
        
        // 处理平移（右键拖拽）
        if resp.secondary_clicked() {
            if let Some(_mouse_pos) = resp.interact_pointer_pos() {
                self.viz_pan += resp.drag_delta();
            }
        }
        
        // 处理缩放（滚轮）
        let scroll_delta = ui.input(|i| i.scroll_delta);
        let zoom_factor = 1.0 + scroll_delta.y * 0.001;
        self.viz_zoom = (self.viz_zoom * zoom_factor).clamp(0.1, 5.0);
        
        // 显示控制提示
        ui.label("🖱️ 左键拖拽场景 | 右键拖拽平移 | 滚轮缩放");
    }
    
    fn draw_screenshot_panel(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::drag());
        if let Some(tex) = &self.texture {
            let painter_size = resp.rect.size();
            let scale = (painter_size.x / self.img_size.x).min(painter_size.y / self.img_size.y);
            let draw_size = self.img_size * scale;
            let draw_rect = Rect::from_min_size(resp.rect.min, draw_size);
            painter.image(tex.id(), draw_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

            let to_screen = |p: Pos2| Pos2::new(
                draw_rect.min.x + p.x * scale,
                draw_rect.min.y + p.y * scale
            );
            let from_screen = |p: Pos2| Pos2::new(
                (p.x - draw_rect.min.x) / scale,
                (p.y - draw_rect.min.y) / scale
            );

            for d in &self.current_scene().drafts {
                let color = match d.kind {
                    ElementKind::TextAnchor{..} => Color32::GREEN,
                    ElementKind::ColorAnchor{..} => Color32::from_rgb(255, 165, 0),
                    ElementKind::Button{..} => Color32::BLUE,
                };
                painter.rect_stroke(Rect::from_min_max(to_screen(d.pos_or_rect.min), to_screen(d.pos_or_rect.max)), 2.0, Stroke::new(2.0, color));
            }

            if resp.drag_started() {
                if let Some(p) = resp.interact_pointer_pos() { self.start_pos = Some(from_screen(p)); }
            }
            if let (Some(start), Some(curr_raw)) = (self.start_pos, resp.interact_pointer_pos()) {
                let curr = from_screen(curr_raw);
                let rect = if self.is_color_picker_mode { Rect::from_min_max(curr, curr + Vec2::splat(1.0)) } else { Rect::from_two_pos(start, curr) };
                painter.rect_stroke(Rect::from_min_max(to_screen(rect.min), to_screen(rect.max)), 0.0, Stroke::new(1.5, Color32::RED));
                if resp.drag_released() { 
                    self.current_rect = Some(rect); 
                    self.start_pos = None; 
                    self.ocr_test_result.clear(); 
                }
            }
        } else {
            ui.centered_and_justified(|ui| ui.label("点击左侧『3秒延时截图』开始工作"));
        }
    }
    
    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_size = 20.0 * self.viz_zoom;
        let start_x = (self.viz_pan.x % grid_size) + rect.min.x;
        let start_y = (self.viz_pan.y % grid_size) + rect.min.y;
        
        for x in (start_x as i32..rect.right() as i32).step_by(grid_size as usize) {
            painter.line_segment(
                [Pos2::new(x as f32, rect.top()), Pos2::new(x as f32, rect.bottom())],
                Stroke::new(0.5, Color32::from_rgb(220, 220, 220))
            );
        }
        
        for y in (start_y as i32..rect.bottom() as i32).step_by(grid_size as usize) {
            painter.line_segment(
                [Pos2::new(rect.left(), y as f32), Pos2::new(rect.right(), y as f32)],
                Stroke::new(0.5, Color32::from_rgb(220, 220, 220))
            );
        }
    }
    
    fn draw_scene_connections(&self, painter: &egui::Painter, transform: &dyn Fn(Pos2) -> Pos2) {
        for scene in self.scenes.iter() {
            let from_rect = Rect::from_min_size(transform(scene.viz_pos), scene.viz_size * self.viz_zoom);
            let from_center = from_rect.center();
            
            for draft in &scene.drafts {
                if let ElementKind::Button { target, .. } = &draft.kind {
                    if let Some(target_idx) = self.scenes.iter().position(|s| s.id == *target) {
                        let target_scene = &self.scenes[target_idx];
                        let to_rect = Rect::from_min_size(transform(target_scene.viz_pos), target_scene.viz_size * self.viz_zoom);
                        let to_center = to_rect.center();
                        
                        // 绘制连接线
                        painter.line_segment(
                            [from_center, to_center],
                            Stroke::new(2.0, Color32::from_rgb(100, 100, 200))
                        );
                        
                        // 绘制箭头
                        let direction = (to_center - from_center).normalized();
                        let arrow_size = 10.0 * self.viz_zoom;
                        let arrow_tip = to_center - direction * (target_scene.viz_size.x * self.viz_zoom / 2.0 + 5.0);
                        
                        let perp = Vec2::new(-direction.y, direction.x) * (arrow_size * 0.5);
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                arrow_tip,
                                arrow_tip - direction * arrow_size + perp,
                                arrow_tip - direction * arrow_size - perp
                            ],
                            Color32::from_rgb(100, 100, 200),
                            Stroke::new(1.0, Color32::from_rgb(100, 100, 200))
                        ));
                    }
                }
            }
        }
    }
} // 🔥 MapBuilderTool 实现块结束

// ==========================================
// 3. UI 实现
// ==========================================
fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(data) = fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
        fonts.font_data.insert("msyh".to_owned(), egui::FontData::from_owned(data));
        fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "msyh".to_owned());
        fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "msyh".to_owned());
    }
    ctx.set_fonts(fonts);
}

impl eframe::App for MapBuilderTool {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(start_time) = self.capture_timer {
            if start_time.elapsed().as_secs_f32() >= 3.0 {
                self.capture_immediate(ctx);
                self.capture_timer = None; 
                self.current_rect = None;
            } else {
                ctx.request_repaint(); 
            }
        }

        egui::SidePanel::left("side").min_width(400.0).show(ctx, |ui| {
            ui.heading("🚀 MINKE UI 建模器 (OCR测试)");
            ui.label(RichText::new(&self.status_msg).color(Color32::from_rgb(0, 255, 128))); 
            ui.add_space(5.0);
            
            ui.group(|ui| {
                if self.capture_timer.is_some() {
                    let remaining = 3.0 - self.capture_timer.unwrap().elapsed().as_secs_f32();
                    ui.add(egui::ProgressBar::new(remaining / 3.0).text(format!("倒计时：{:.1}s", remaining)));
                } else {
                    if ui.button("📸 3秒延时截图").clicked() { self.capture_timer = Some(Instant::now()); }
                }
            });

            // --- 视图切换 --- 
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("视图模式:");
                ui.radio_value(&mut self.show_visualization, false, "截图编辑");
                ui.radio_value(&mut self.show_visualization, true, "场景可视化");
            });

            if !self.show_visualization {
                // --- 场景管理 --- 
                ui.separator();
                ui.heading("🎬 场景管理");
                ui.horizontal(|ui| {
                    if ui.button("➕ 新建场景").clicked() { self.add_new_scene(); }
                    if ui.button("📋 复制场景").clicked() { self.duplicate_current_scene(); }
                    if ui.button("❌ 删除场景").clicked() { self.delete_current_scene(); }
                });
                
                egui::ScrollArea::vertical().id_source("scene_list").max_height(150.0).show(ui, |ui| {
                    for (i, scene) in self.scenes.iter().enumerate() {
                        let is_active = i == self.current_scene_index;
                        let mut button_text = format!("{}. {}", i + 1, scene.name);
                        if scene.handler.is_some() {
                            button_text.push_str(&format!(" (handler: {})", scene.handler.as_ref().unwrap()));
                        }
                        
                        let response = ui.selectable_label(is_active, button_text);
                        if response.clicked() {
                            self.current_scene_index = i;
                            self.status_msg = format!("已切换到场景：{}", scene.name);
                        }
                    }
                });

                // --- 当前场景编辑 --- 
                ui.separator();
                ui.heading("📝 场景属性");
                
                { // 作用域限制可变借用
                    let current_scene = self.current_scene_mut();
                    ui.horizontal(|ui| { ui.label("ID:"); ui.text_edit_singleline(&mut current_scene.id); });
                    ui.horizontal(|ui| { ui.label("名称:"); ui.text_edit_singleline(&mut current_scene.name); });
                    ui.horizontal(|ui| { 
                        ui.label("逻辑:"); 
                        ui.radio_value(&mut current_scene.logic, RecognitionLogic::AND, "AND"); 
                        ui.radio_value(&mut current_scene.logic, RecognitionLogic::OR, "OR"); 
                    });
                    ui.horizontal(|ui| { ui.label("Handler:"); ui.text_edit_singleline(current_scene.handler.get_or_insert(String::new())); });
                }

                ui.separator();
                ui.checkbox(&mut self.is_color_picker_mode, "🧪 吸管取色模式");

                if let Some(rect) = self.current_rect {
                    ui.group(|ui| {
                        ui.label(RichText::new("已选中目标：").color(Color32::from_rgb(0, 255, 255)).strong());
                        
                        if self.is_color_picker_mode {
                            let color = self.pick_color(rect.min);
                            ui.label(format!("HEX: {}", color));
                            if ui.button("📌 添加颜色锚点").clicked() {
                                let current_scene = self.current_scene_mut();
                                current_scene.drafts.push(UIElementDraft { pos_or_rect: rect, kind: ElementKind::ColorAnchor { color_hex: color, tolerance: 15 } });
                                self.current_rect = None;
                            }
                        } else {
                            ui.horizontal(|ui| {
                                if ui.button("⚓ 添加 Text 锚点").clicked() {
                                    let val = if self.ocr_test_result.is_empty() || self.ocr_test_result.contains("...") { "Text".to_string() } else { self.ocr_test_result.clone() };
                                    let current_scene = self.current_scene_mut();
                                    current_scene.drafts.push(UIElementDraft { pos_or_rect: rect, kind: ElementKind::TextAnchor { text: val } });
                                    self.current_rect = None;
                                }
                                if ui.button("🔍 区域 OCR 测试").clicked() {
                                    self.perform_ocr(rect);
                                }
                            });
                            
                            if !self.ocr_test_result.is_empty() {
                                ui.label(RichText::new(format!("识别结果: [{}]", self.ocr_test_result)).color(Color32::BLACK));
                            }

                            if ui.button("🖱️ 添加 Button 跳转").clicked() {
                                let current_scene = self.current_scene_mut();
                                current_scene.drafts.push(UIElementDraft { pos_or_rect: rect, kind: ElementKind::Button { target: "next".into(), post_delay: 500 } });
                                self.current_rect = None;
                            }
                        }
                    });
                }

                // --- 元素列表 --- 
                ui.separator();
                ui.heading("📋 元素列表");
                egui::ScrollArea::vertical().id_source("element_list").max_height(200.0).show(ui, |ui| {
                    let current_scene = self.current_scene_mut();
                    let mut del = None;
                    for (i, d) in current_scene.drafts.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            match &mut d.kind {
                                ElementKind::TextAnchor { text } => { ui.label("⚓"); ui.text_edit_singleline(text); }
                                ElementKind::ColorAnchor { color_hex, tolerance } => {
                                    ui.label("🧪"); ui.label(color_hex.as_str());
                                    ui.add(egui::DragValue::new(tolerance).prefix("T:"));
                                }
                                ElementKind::Button { target, post_delay } => {
                                    ui.label("🖱️"); ui.text_edit_singleline(target);
                                    ui.add(egui::DragValue::new(post_delay).prefix("ms:"));
                                }
                            }
                            if ui.button("❌").clicked() { del = Some(i); }
                        });
                    }
                    if let Some(i) = del { current_scene.drafts.remove(i); }
                });
            }

            // --- TOML 操作 --- 
            ui.separator();
            ui.heading("📄 TOML 操作");
            ui.horizontal(|ui| {
                if ui.button("📤 生成 TOML").clicked() { self.build_toml(); }
                if ui.button("📥 导入 TOML").clicked() { self.import_toml(); }
                if ui.button("💾 保存到文件").clicked() {
                    let file_path = "./ui_map.toml";
                    if let Ok(_) = std::fs::write(file_path, &self.toml_content) {
                        self.status_msg = format!("已保存到 {}", file_path).into();
                    } else {
                        self.status_msg = "保存文件失败".into();
                    }
                }
                if ui.button("📂 加载文件").clicked() {
                    let file_path = "./ui_map.toml";
                    if let Ok(content) = std::fs::read_to_string(file_path) {
                        self.toml_content = content;
                        self.import_toml();
                        self.status_msg = format!("已加载 {}", file_path).into();
                    } else {
                        self.status_msg = "加载文件失败".into();
                    }
                }
            });
            
            egui::ScrollArea::vertical().id_source("toml_scroll").show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.toml_content).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.show_visualization {
                // 场景可视化模式
                self.draw_visualization_panel(ui);
            } else {
                // 截图编辑模式
                self.draw_screenshot_panel(ui);
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]), ..Default::default() };
    eframe::run_native("MINKE UI Mapper Pro", opts, Box::new(|cc| Box::new(MapBuilderTool::new(cc))))
}