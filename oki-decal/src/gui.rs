// oki-decal-gui: print white-toner decals for OKI C711WT — egui GUI
// in release: nessuna console sotto la finestra
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use eframe::egui;
use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use image::GenericImageView;
use oki_decal::{decal_stream, white_mask_from_alpha, DecalOptions};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([880.0, 680.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("oki-decal  ·  OKI C711WT white toner"),
        ..Default::default()
    };
    eframe::run_native("oki-decal", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

// ---------------------------------------------------------------- app state
struct App {
    path: Option<String>,
    texture: Option<TextureHandle>,
    tex_size: (u32, u32),
    orig_size: (u32, u32),
    opts: DecalOptions,
    highlight_white: bool,
    printer: String,
    status: Status,
    rx: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
}

enum Status {
    Idle(&'static str),
    Info(String),
    Ok(String),
    Warn(String),
    Err(String),
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark); // ignore system light theme
        style(&cc.egui_ctx);
        App {
            path: None,
            texture: None,
            tex_size: (1, 1),
            orig_size: (1, 1),
            opts: DecalOptions::default(),
            highlight_white: false,
            printer: "OKI C711WT".into(),
            status: Status::Idle("Open or drop a transparent PNG to begin"),
            rx: None,
        }
    }

    // ---- preview: image on dark checkerboard (white pops naturally);
    //      optional cyan tint marking exactly where the WHITE toner goes
    fn load_preview(&mut self, ctx: &egui::Context, path: &str, explicit: bool, tint: bool) {
        let img = match image::open(path) {
            Ok(i) => i,
            Err(e) => {
                self.status = Status::Err(format!("Cannot read image: {e}"));
                return;
            }
        };
        let (w, h) = img.dimensions();
        let scale = (760.0f32 / w.max(h) as f32).min(1.0f32);
        let pw = (w as f32 * scale).round().max(1.0) as u32;
        let ph = (h as f32 * scale).round().max(1.0) as u32;
        let small = img.resize_exact(pw, ph, image::imageops::FilterType::Lanczos3);
        let mask = white_mask_from_alpha(&small, 0, explicit);

        let rgba = small.to_rgba8();
        let has_alpha = rgba.pixels().any(|p| p.0[3] < 250);
        let mut px = Vec::with_capacity((pw * ph * 4) as usize);
        for (i, p) in rgba.pixels().enumerate() {
            let [r, g, b, a] = p.0;
            let (x, y) = ((i % pw as usize) as u32, (i / pw as usize) as u32);
            let checker = ((x / 10 + y / 10) % 2 == 0) as u8;
            let bg = 195 + checker * 40; // checker chiaro stile Photoshop
            let af = a as f32 / 255.0;
            let mut rr = (r as f32 * af + bg as f32 * (1.0 - af)) as u8;
            let mut gg = (g as f32 * af + bg as f32 * (1.0 - af)) as u8;
            let mut bb = (b as f32 * af + bg as f32 * (1.0 - af)) as u8;
            if tint && mask[i] {
                rr = (rr as f32 * 0.15) as u8;
                gg = (gg as f32 * 0.15 + 170.0 * 0.85) as u8;
                bb = (bb as f32 * 0.15 + 255.0 * 0.85) as u8;
            }
            px.extend_from_slice(&[rr, gg, bb]);
        }
        let img = ColorImage::from_rgb([pw as usize, ph as usize], &px);
        self.texture = Some(ctx.load_texture("preview", img, TextureOptions::LINEAR));
        self.tex_size = (pw, ph);
        self.orig_size = (w, h);
        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = if !has_alpha {
            Status::Warn(format!(
                "{name}: no transparency in this file (JPG?) — white would go everywhere. Use a transparent PNG, or switch to 'Where I painted white'."
            ))
        } else {
            Status::Info(format!("{name}  ·  {w}×{h}px"))
        };
    }

    fn reload_preview(&mut self, ctx: &egui::Context) {
        if let Some(p) = self.path.clone() {
            let (e, t) = (self.opts.explicit_white, self.highlight_white);
            self.load_preview(ctx, &p, e, t);
        }
    }

    fn print(&mut self) {
        if self.rx.is_some() {
            return; // gia' in elaborazione
        }
        let path = match &self.path {
            Some(p) => p.clone(),
            None => {
                self.status = Status::Warn("No image loaded".into());
                return;
            }
        };
        let opts = self.opts.clone();
        let printer = self.printer.clone();
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = Status::Info(format!("Processing {name}: ICC → curves → halftone → PCL…"));
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let res = match decal_stream(&path, &opts) {
                Ok(bytes) => {
                    let _ = std::fs::write("decal.prn", &bytes); // safety copy
                    match raw_print(&printer, &bytes) {
                        Ok(n) => Ok(format!("Sent! {n} bytes → '{printer}'")),
                        Err(e) => Err(format!(
                            "decal.prn saved ({} B) but send failed: {e} — fallback: test/Send-RAW.ps1",
                            bytes.len()
                        )),
                    }
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(res);
        });
    }
}

// ---------------------------------------------------------------- raw print (winspool)
fn raw_print(printer: &str, bytes: &[u8]) -> Result<usize, String> {
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Printing::*;

    let pname = HSTRING::from(printer);
    let hdoc = HSTRING::from("oki-decal");
    let hraw = HSTRING::from("RAW");
    let mut h = HANDLE::default();
    unsafe {
        OpenPrinterW(&pname, &mut h, None).map_err(|e| format!("queue «{printer}» not found: {e}"))?;
        let doc = DOC_INFO_1W {
            pDocName: PWSTR::from_raw(hdoc.as_wide().as_ptr() as *mut u16),
            pOutputFile: PWSTR::null(),
            pDatatype: PWSTR::from_raw(hraw.as_wide().as_ptr() as *mut u16),
        };
        let res = (|| -> Result<usize, String> {
            if StartDocPrinterW(h, 1, &doc) == 0 {
                return Err("StartDocPrinter failed".into());
            }
            let _ = StartPagePrinter(h);
            let mut written: u32 = 0;
            if !WritePrinter(h, bytes.as_ptr() as *const _, bytes.len() as u32, &mut written).as_bool() {
                return Err("WritePrinter failed".into());
            }
            Ok(written as usize)
        })();
        let _ = EndPagePrinter(h);
        let _ = EndDocPrinter(h);
        let _ = ClosePrinter(h);
        res
    }
}

// ---------------------------------------------------------------- theme
fn style(ctx: &egui::Context) {
    let mut s = egui::Style::default();
    s.visuals = egui::Visuals::dark();
    s.visuals.panel_fill = Color32::from_rgb(22, 24, 30);
    s.visuals.window_fill = Color32::from_rgb(16, 17, 22);
    s.visuals.extreme_bg_color = Color32::from_rgb(12, 13, 17);
    s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(44, 48, 58);
    s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(58, 64, 78);
    s.visuals.widgets.active.bg_fill = Color32::from_rgb(66, 73, 88);
    s.visuals.selection.bg_fill = Color32::from_rgb(70, 110, 180);
    s.visuals.override_text_color = Some(Color32::from_rgb(225, 230, 240));
    ctx.set_style(s);
}

// ---------------------------------------------------------------- ui
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // risultato del thread di stampa (se attivo)
        if let Some(rx) = &self.rx {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    self.status = Status::Ok(msg);
                    self.rx = None;
                }
                Ok(Err(msg)) => {
                    self.status = Status::Warn(msg);
                    self.rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                }
            }
        }

        // drag & drop
        let dropped: Option<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .find_map(|f| f.path.clone())
                .map(|p| p.to_string_lossy().into_owned())
        });
        if let Some(p) = dropped {
            self.path = Some(p.clone());
            let (e, t) = (self.opts.explicit_white, self.highlight_white);
            self.load_preview(ctx, &p, e, t);
        }

        // bottom: status bar
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                let (txt, color) = match &self.status {
                    Status::Idle(s) => (*s, Color32::GRAY),
                    Status::Info(s) => (s.as_str(), Color32::from_rgb(160, 175, 200)),
                    Status::Ok(s) => (s.as_str(), Color32::from_rgb(120, 220, 130)),
                    Status::Warn(s) => (s.as_str(), Color32::from_rgb(240, 190, 90)),
                    Status::Err(s) => (s.as_str(), Color32::from_rgb(240, 110, 110)),
                };
                ui.colored_label(color, txt);
            });
        });

        // right: controls
        egui::SidePanel::right("controls")
            .resizable(false)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("oki-decal");
                ui.label(
                    egui::RichText::new("OKI C711WT · white toner decals")
                        .small()
                        .color(Color32::from_rgb(130, 145, 170)),
                );
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(6.0);

                ui.label(egui::RichText::new("QUEUE").small().strong().color(Color32::from_rgb(200, 210, 230)));
                let mut queue_changed = false;
                let mut mode_changed = false;
                if ui.radio_value(&mut self.opts.photo_mode, false, "🎨 Decals (white toner)").changed() { queue_changed = true; }
                if ui.radio_value(&mut self.opts.photo_mode, true, "📷 Photo (CMY only)").changed() { queue_changed = true; }
                if queue_changed && self.opts.photo_mode {
                    self.opts.dots45 = false; // le foto stanno meglio col diffusion
                }
                if self.opts.photo_mode {
                    ui.checkbox(&mut self.opts.k_toner, "Black toner installed (real K)");
                    ui.label(
                        egui::RichText::new("CMY only (or CMYK with black toner). Darks are CMY composite without K.")
                            .small()
                            .color(Color32::from_rgb(150, 160, 185)),
                    );
                }
                ui.add_space(10.0);

                if !self.opts.photo_mode {
                    ui.label(egui::RichText::new("WHITE").small().strong().color(Color32::from_rgb(200, 210, 230)));
                    if ui
                        .radio_value(&mut self.opts.explicit_white, false, "Underbase (whole shape)")
                        .changed()
                    {
                        mode_changed = true;
                    }
                    if ui
                        .radio_value(&mut self.opts.explicit_white, true, "Only where I painted white")
                        .changed()
                    {
                        mode_changed = true;
                    }
                    if ui.checkbox(&mut self.highlight_white, "Highlight white (cyan)").changed() {
                        mode_changed = true;
                    }
                    ui.add_space(4.0);
                    if ui.checkbox(&mut self.opts.double_white, "DOUBLE WHITE (2nd white in cyan slot)").changed() {
                        mode_changed = true;
                    }
                    if self.opts.double_white {
                        ui.label(
                            egui::RichText::new("2x white density — CYAN disabled!\nRequires 2nd white toner in cyan slot.")
                                .small()
                                .color(Color32::from_rgb(240, 160, 90)),
                        );
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("on dark background white shows as-is")
                            .small()
                            .color(Color32::from_rgb(120, 135, 160)),
                    );
                    ui.add_space(10.0);
                }

                ui.checkbox(&mut self.opts.mirror, "↔  Mirror");
                ui.add_space(10.0);

                ui.label(egui::RichText::new("SIZE & POSITION (mm)").small().strong().color(Color32::from_rgb(200, 210, 230)));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label("W");
                    ui.add(egui::DragValue::new(&mut self.opts.width_mm).speed(1.0).range(0.0..=200.0).suffix(" mm"));
                    ui.label("(0 = fit)");
                });
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut self.opts.x_mm).speed(1.0).range(0.0..=200.0).suffix(" mm"));
                    ui.label("Y");
                    ui.add(egui::DragValue::new(&mut self.opts.y_mm).speed(1.0).range(0.0..=280.0).suffix(" mm"));
                });
                ui.add_space(4.0);
                ui.label(egui::RichText::new("SCREEN").small().strong().color(Color32::from_rgb(200, 210, 230)));
                ui.radio_value(&mut self.opts.dots45, true, "Dots 45° (flat colors)");
                ui.radio_value(&mut self.opts.dots45, false, "Diffusion (photos)");
                ui.add_space(10.0);

                ui.label(egui::RichText::new("PAPER").small().strong().color(Color32::from_rgb(200, 210, 230)));
                egui::ComboBox::from_id_salt("paper")
                    .selected_text(self.opts.page.clone())
                    .width(ui.available_width() - 12.0)
                    .show_ui(ui, |ui| {
                        for (m, hint) in [
                            ("a4", "A4 210×297"),
                            ("letter", "Letter 216×279"),
                            ("legal", "Legal 216×356"),
                            ("a5", "A5 148×210"),
                            ("a3", "A3 297×420"),
                            ("11x17", "11×17 279×432"),
                            ("a4ls", "A4 landscape"),
                            ("letterls", "Letter landscape"),
                        ] {
                            ui.selectable_value(&mut self.opts.page, m.to_string(), format!("{m} — {hint}"));
                        }
                    });
                ui.add_space(10.0);

                ui.label(egui::RichText::new("MEDIA").small().strong().color(Color32::from_rgb(200, 210, 230)));
                egui::ComboBox::from_id_salt("media")
                    .selected_text(self.opts.media.clone())
                    .width(ui.available_width() - 12.0)
                    .show_ui(ui, |ui| {
                        for (m, hint) in [
                            ("glossy", "decal film / glossy"),
                            ("labels", "labels (thin)"),
                            ("transparency", "transparency"),
                            ("cardstock", "card stock (slow fuse)"),
                            ("plain", "plain paper"),
                            ("bond", "bond"),
                        ] {
                            ui.selectable_value(&mut self.opts.media, m.to_string(), format!("{m} — {hint}"));
                        }
                    });
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(6.0);

                ui.label(egui::RichText::new("PRINTER QUEUE").small().strong().color(Color32::from_rgb(200, 210, 230)));
                ui.add_space(2.0);
                ui.text_edit_singleline(&mut self.printer);
                ui.add_space(12.0);

                let can_print = self.path.is_some();
                let b_open = ui.add(
                    egui::Button::new("📂  Open image…")
                        .min_size(egui::vec2(ui.available_width() - 8.0, 32.0)),
                );
                if b_open.clicked() {
                    if let Some(f) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"])
                        .pick_file()
                    {
                        self.path = Some(f.to_string_lossy().into_owned());
                        self.reload_preview(ctx);
                    }
                }
                ui.add_space(6.0);

                let busy = self.rx.is_some();
                let b_print = ui.add_enabled(
                    can_print && !busy,
                    egui::Button::new(
                        egui::RichText::new(if busy {
                            "⏳  PROCESSING…".to_string()
                        } else {
                            "🖨   P R I N T".to_string()
                        })
                        .color(if can_print && !busy {
                            Color32::BLACK
                        } else {
                            Color32::GRAY
                        })
                        .strong(),
                    )
                    .fill(if can_print && !busy {
                        Color32::from_rgb(120, 190, 130)
                    } else {
                        Color32::from_rgb(70, 76, 88)
                    })
                    .min_size(egui::vec2(ui.available_width() - 8.0, 42.0)),
                );
                if b_print.clicked() {
                    self.print();
                }
                ui.add_space(6.0);
                if ui
                    .add(
                        egui::Button::new("💾  Save decal.prn only (no print)")
                            .min_size(egui::vec2(ui.available_width() - 8.0, 26.0)),
                    )
                    .clicked()
                {
                    if let Some(p) = &self.path {
                        match decal_stream(p, &self.opts) {
                            Ok(b) => {
                                let _ = std::fs::write("decal.prn", &b);
                                self.status = Status::Ok(format!("decal.prn saved — {} KB", b.len() / 1024));
                            }
                            Err(e) => self.status = Status::Err(e),
                        }
                    }
                }
                if mode_changed {
                    self.reload_preview(ctx);
                }
            });

        // center: A4 page preview with draggable image
        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_size();
            let page_h = (avail.y - 30.0).max(100.0);
            let (_pcode, pw_in, ph_in, _land) = oki_decal::page_spec(&self.opts.page);
            let paper_w_mm = pw_in as f32 * 25.4;
            let paper_h_mm = ph_in as f32 * 25.4;
            let mut page_w = page_h * (paper_w_mm / paper_h_mm);
            if page_w > avail.x - 30.0 {
                page_w = avail.x - 30.0;
            }
            let page_h = page_w * (paper_h_mm / paper_w_mm);
            let px_mm = page_h / paper_h_mm;

            let (page_rect, _resp) = ui.allocate_exact_size(egui::vec2(page_w, page_h), egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(page_rect.translate(egui::vec2(4.0, 5.0)), 3.0, Color32::from_rgb(8, 9, 12));
            painter.rect_filled(page_rect, 3.0, Color32::from_rgb(250, 250, 250));
            painter.rect_stroke(page_rect, 0.0, egui::Stroke::new(1.0, Color32::from_rgb(60, 64, 74)));

            if let Some(tex) = &self.texture {
                let (iw_px, ih_px) = self.orig_size;
                // mm naturali a 600 dpi — dall'immagine ORIGINALE (non dalla preview ridotta!)
                let nat_w_mm = iw_px as f32 * 25.4 / 600.0;
                let nat_h_mm = ih_px as f32 * 25.4 / 600.0;
                let max_w_mm = 210.0 - 50.0;
                let max_h_mm = 297.0 - 50.0;
                let (w_mm, h_mm) = if self.opts.width_mm > 0.0 {
                    let w = self.opts.width_mm;
                    (w, nat_h_mm * (w / nat_w_mm))
                } else {
                    let s = (max_w_mm / nat_w_mm).min(max_h_mm / nat_h_mm).min(1.0);
                    (nat_w_mm * s, nat_h_mm * s)
                };
                let img_rect = egui::Rect::from_min_size(
                    page_rect.min + egui::vec2(self.opts.x_mm * px_mm, self.opts.y_mm * px_mm),
                    egui::vec2(w_mm * px_mm, h_mm * px_mm),
                );
                painter.image(tex.id(), img_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);

                let resp = ui.interact(img_rect, egui::Id::new("img_drag"), egui::Sense::drag());
                if resp.hovered() || resp.dragged() {
                    painter.rect_stroke(img_rect, 0.0, egui::Stroke::new(1.0, Color32::from_rgb(70, 110, 180)));
                }
                if resp.dragged() {
                    self.opts.x_mm = (self.opts.x_mm + resp.drag_delta().x / px_mm).clamp(0.0, (paper_w_mm - w_mm).max(0.0));
                    self.opts.y_mm = (self.opts.y_mm + resp.drag_delta().y / px_mm).clamp(0.0, (paper_h_mm - h_mm).max(0.0));
                }
            } else {
                painter.text(
                    page_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Drop a transparent PNG here",
                    egui::FontId::proportional(15.0),
                    Color32::from_rgb(150, 155, 170),
                );
            }

            painter.text(
                page_rect.left_bottom() + egui::vec2(0.0, 16.0),
                egui::Align2::LEFT_TOP,
                format!("X {:.1} mm   Y {:.1} mm", self.opts.x_mm, self.opts.y_mm),
                egui::FontId::proportional(12.0),
                Color32::from_rgb(140, 150, 170),
            );
        });
    }
}
