// oki-decal-gui: stampare decals OKI C711WT con toner bianco — GUI egui
use eframe::egui;
use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use image::GenericImageView;
use oki_decal::{decal_stream, white_mask_from_alpha, DecalOptions};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 660.0])
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
    opts: DecalOptions,
    printer: String,
    status: Status,
    dragging_over: bool,
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
        style(&cc.egui_ctx);
        App {
            path: None,
            texture: None,
            tex_size: (1, 1),
            opts: DecalOptions::default(),
            printer: "OKI C711WT".into(),
            status: Status::Idle("Apri o trascina una PNG con trasparenza"),
            dragging_over: false,
        }
    }

    // ---- preview: immagine su scacchiera + tint azzurro dove stampera' il BIANCO
    fn load_preview(&mut self, ctx: &egui::Context, path: &str, explicit: bool) {
        let img = match image::open(path) {
            Ok(i) => i,
            Err(e) => {
                self.status = Status::Err(format!("Immagine non leggibile: {e}"));
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
        // il file ha trasparenza? (se no: avviso — i JPG non hanno canale alpha)
        let has_alpha = rgba.pixels().any(|p| p.0[3] < 250);
        let mut px = Vec::with_capacity((pw * ph * 4) as usize);
        for (i, p) in rgba.pixels().enumerate() {
            let [r, g, b, a] = p.0;
            let (x, y) = ((i % pw as usize) as u32, (i / pw as usize) as u32);
            let checker = ((x / 10 + y / 10) % 2 == 0) as u8;
            let bg = 96 + checker * 44;
            let af = a as f32 / 255.0;
            let mut rr = (r as f32 * af + bg as f32 * (1.0 - af)) as u8;
            let mut gg = (g as f32 * af + bg as f32 * (1.0 - af)) as u8;
            let mut bb = (b as f32 * af + bg as f32 * (1.0 - af)) as u8;
            if mask[i] {
                rr = (rr as f32 * 0.22) as u8;
                gg = (gg as f32 * 0.22 + 184.0 * 0.78) as u8;
                bb = (bb as f32 * 0.22 + 255.0 * 0.78) as u8;
            }
            px.extend_from_slice(&[rr, gg, bb]);
        }
        let img = ColorImage::from_rgb([pw as usize, ph as usize], &px);
        self.texture = Some(ctx.load_texture("preview", img, TextureOptions::LINEAR));
        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = if !has_alpha {
            Status::Warn(format!(
                "{name}: nessuna trasparenza (JPG?) → qui il BIANCO andrebbe ovunque. Usa una PNG con sfondo trasparente, o passa a «Dove ho dipinto bianco»"
            ))
        } else {
            Status::Info(format!("{name}  ·  {w}×{h}px  ·  azzurro = zona BIANCO"))
        };
        self.tex_size = (pw, ph);
        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = Status::Info(format!("{name}  ·  {w}×{h}px  ·  azzurro = zona BIANCO"));
    }

    fn reload_preview(&mut self, ctx: &egui::Context) {
        if let Some(p) = self.path.clone() {
            let e = self.opts.explicit_white;
            self.load_preview(ctx, &p, e);
        }
    }

    fn print(&mut self) {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => {
                self.status = Status::Warn("Nessuna immagine caricata".into());
                return;
            }
        };
        let opts = self.opts.clone();
        self.status = Status::Info("Elaborazione: ICC → curve → halftone → PCL…".into());
        match decal_stream(&path, &opts) {
            Ok(bytes) => {
                let _ = std::fs::write("decal.prn", &bytes); // copia di sicurezza
                match raw_print(&self.printer, &bytes) {
                    Ok(n) => self.status = Status::Ok(format!("Stampato! {n} byte → «{}»", self.printer)),
                    Err(e) => self.status = Status::Warn(format!(
                        "decal.prn salvato ({} B) ma invio fallito: {e} — fallback: powershell -File test/Send-RAW.ps1",
                        bytes.len()
                    )),
                }
            }
            Err(e) => self.status = Status::Err(e),
        }
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
        OpenPrinterW(&pname, &mut h, None).map_err(|e| format!("coda «{printer}» non trovata: {e}"))?;
        let doc = DOC_INFO_1W {
            pDocName: PWSTR::from_raw(hdoc.as_wide().as_ptr() as *mut u16),
            pOutputFile: PWSTR::null(),
            pDatatype: PWSTR::from_raw(hraw.as_wide().as_ptr() as *mut u16),
        };
        let res = (|| -> Result<usize, String> {
            if StartDocPrinterW(h, 1, &doc) == 0 {
                return Err("StartDocPrinter fallita".into());
            }
            let _ = StartPagePrinter(h);
            let mut written: u32 = 0;
            if !WritePrinter(h, bytes.as_ptr() as *const _, bytes.len() as u32, &mut written).as_bool()
            {
                return Err("WritePrinter fallita".into());
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
    s.visuals.panel_fill = Color32::from_rgb(24, 26, 32);
    s.visuals.window_fill = Color32::from_rgb(30, 33, 40);
    s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(42, 46, 56);
    s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(56, 62, 76);
    s.visuals.widgets.active.bg_fill = Color32::from_rgb(64, 70, 86);
    s.visuals.selection.bg_fill = Color32::from_rgb(70, 110, 180);
    ctx.set_style(s);
}

// ---------------------------------------------------------------- ui
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
            let e = self.opts.explicit_white;
            self.load_preview(ctx, &p, e);
        }

        // bottom: status
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(4.0);
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
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("🖨  oki-decal");
                ui.label(
                    egui::RichText::new("OKI C711WT · toner bianco")
                        .small()
                        .color(Color32::from_rgb(130, 145, 170)),
                );
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(6.0);

                ui.label(egui::RichText::new("BIANCO").small().strong().color(Color32::from_rgb(200, 210, 230)));
                let mut mode_changed = false;
                if ui
                    .radio_value(&mut self.opts.explicit_white, false, "Silhouette sotto tutto")
                    .changed()
                {
                    mode_changed = true;
                }
                if ui
                    .radio_value(&mut self.opts.explicit_white, true, "Dove ho dipinto bianco")
                    .changed()
                {
                    mode_changed = true;
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("azzurro in anteprima = toner bianco")
                        .small()
                        .color(Color32::from_rgb(120, 135, 160)),
                );
                ui.add_space(10.0);

                ui.checkbox(&mut self.opts.mirror, "↔  Mirror");
                ui.add_space(10.0);

                ui.label(egui::RichText::new("SUPPORTO").small().strong().color(Color32::from_rgb(200, 210, 230)));
                egui::ComboBox::from_id_salt("media")
                    .selected_text(self.opts.media.clone())
                    .width(ui.available_width() - 12.0)
                    .show_ui(ui, |ui| {
                        for (m, hint) in [
                            ("glossy", "film decal / glossy"),
                            ("labels", "etichette"),
                            ("transparency", "trasparenze"),
                            ("cardstock", "cartoncino (fusione lenta)"),
                            ("plain", "carta normale"),
                            ("bond", "bond"),
                        ] {
                            ui.selectable_value(&mut self.opts.media, m.to_string(), format!("{m} — {hint}"));
                        }
                    });
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(6.0);

                ui.label(egui::RichText::new("CODA STAMPANTE").small().strong().color(Color32::from_rgb(200, 210, 230)));
                ui.add_space(2.0);
                ui.text_edit_singleline(&mut self.printer);
                ui.add_space(12.0);

                let b_open = ui.add(
                    egui::Button::new("📂  Apri immagine…")
                        .min_size(egui::vec2(ui.available_width() - 8.0, 32.0)),
                );
                if b_open.clicked() {
                    if let Some(f) = rfd::FileDialog::new()
                        .add_filter("Immagini", &["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"])
                        .pick_file()
                    {
                        self.path = Some(f.to_string_lossy().into_owned());
                        self.reload_preview(ctx);
                    }
                }
                ui.add_space(6.0);

                let can_print = self.path.is_some();
                let b_print = ui.add(
                    egui::Button::new(
                        egui::RichText::new("🖨   S T A M P A".to_string())
                            .color(if can_print { Color32::BLACK } else { Color32::GRAY })
                            .strong(),
                    )
                    .fill(if can_print {
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
                        egui::Button::new("💾  solo decal.prn (senza stampare)")
                            .min_size(egui::vec2(ui.available_width() - 8.0, 26.0)),
                    )
                    .clicked()
                {
                    if let Some(p) = &self.path {
                        match decal_stream(p, &self.opts) {
                            Ok(b) => {
                                let _ = std::fs::write("decal.prn", &b);
                                self.status = Status::Ok(format!("decal.prn salvato — {} KB", b.len() / 1024));
                            }
                            Err(e) => self.status = Status::Err(e),
                        }
                    }
                }
                if mode_changed {
                    self.reload_preview(ctx);
                }
            });

        // center: preview
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.texture {
                let avail = ui.available_size();
                let (tw, th) = self.tex_size;
                let s = (avail.x / tw as f32).min(avail.y / th as f32).min(1.4);
                let size = egui::vec2(tw as f32 * s, th as f32 * s);
                ui.centered_and_justified(|ui| {
                    egui::Frame::default()
                        .fill(Color32::from_rgb(18, 19, 24))
                        .inner_margin(egui::Margin::symmetric(6.0, 6.0))
                        .show(ui, |ui| {
                            ui.image((tex.id(), size));
                        });
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(-40.0);
                        ui.label(
                            egui::RichText::new("🖼")
                                .size(52.0)
                                .color(Color32::from_rgb(90, 100, 120)),
                        );
                        ui.add_space(10.0);
                        ui.label("Trascina qui una PNG con trasparenza");
                        ui.label(
                            egui::RichText::new("oppure «Apri immagine…» →")
                                .small()
                                .color(Color32::GRAY),
                        );
                    });
                });
            }
        });
    }
}
