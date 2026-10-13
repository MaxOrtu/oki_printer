// oki-decal: decal printer per OKI C711WT (toner swap fisico: White,Yellow,Magenta,Cyan)
// Genera stream PJL+PCL con 4 piani (3ColorWhite: Cyan,White,Magenta,Yellow)
// Profili ICC Cadlink in profiles/, ricetta in profiles/underprint_live.json

use image::{DynamicImage, GenericImageView, Rgba};
use std::fs;

const ESC: u8 = 0x1B;

// ---------- profilo di stampa ----------
struct Profile {
    xmm: i32,
    ymm: i32,
    media_name: &'static str,   // es. "dGlossy"
    job_title: String,
}

// mappa nome -> comando PCL media type (dal PDX: MediaType)
fn media_code(name: &str) -> &'static str {
    match name.to_lowercase().as_str() {
        "plain" => "dPlain",
        "letterhead" => "dLetterhead",
        "transparency" => "dTransparency",
        "labels" => "dLabels",
        "bond" => "dBond",
        "recycled" => "dRecycled",
        "cardstock" | "card" => "dCard Stock",
        "rough" => "dRough",
        "glossy" | "gloss" => "dGlossy",
        "postcard" => "dPostcard",
        _ => "dGlossy",
    }
}

fn a4_profile(job_title: &str) -> Profile {
    // BBox A4 dal PIE: [0, 11.36, 7.93, 0] pollici; margini 200dot@600 X, 400dot@1200 Y
    let bw = 7.93f64;
    let bh = 11.36f64;
    let xmm = ((bw * 600.0 + 200.0) / 600.0 * 25.4).round() as i32; // ~209
    let ymm = ((bh * 1200.0 + 400.0) / 1200.0 * 25.4).round() as i32; // ~297
    Profile {
        xmm,
        ymm,
        media_name: "dGlossy",
        job_title: job_title.to_string(),
    }
}

// ---------- curve di linearizzazione (13 punti, dal psqx live) ----------
const CURVE_C: [f32; 13] = [0.0000, 0.0532, 0.0911, 0.1646, 0.2408, 0.3032, 0.3715, 0.4631, 0.5873, 0.6669, 0.7599, 0.8116, 1.0000];
const CURVE_M: [f32; 13] = [0.0000, 0.0728, 0.0963, 0.1561, 0.2343, 0.3093, 0.3724, 0.4163, 0.4751, 0.5817, 0.6901, 0.7658, 1.0000];
const CURVE_Y: [f32; 13] = [0.0000, 0.0590, 0.1033, 0.1812, 0.2673, 0.3276, 0.4056, 0.4777, 0.5447, 0.6405, 0.7584, 0.8006, 1.0000];

fn apply_curve(v: f32, curve: &[f32; 13]) -> f32 {
    let x = v.clamp(0.0, 1.0) * 12.0;
    let i = (x as usize).min(11);
    let f = x - i as f32;
    curve[i] + (curve[i + 1] - curve[i]) * f
}

// ---------- TIFF/PackBits (PCL compression mode 2) ----------
fn packbits(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(row.len() + row.len() / 8 + 1);
    let n = row.len();
    let mut i = 0;
    while i < n {
        let mut run = 1usize;
        while i + run < n && run < 128 && row[i + run] == row[i] {
            run += 1;
        }
        if run >= 2 {
            out.push((257i32 - run as i32) as u8);
            out.push(row[i]);
            i += run;
        } else {
            let start = i;
            let mut lit = 1usize;
            i += 1;
            while i < n && lit < 128 {
                if i + 2 < n && row[i] == row[i + 1] && row[i + 1] == row[i + 2] {
                    break;
                }
                lit += 1;
                i += 1;
            }
            out.push((lit - 1) as u8);
            out.extend_from_slice(&row[start..start + lit]);
        }
    }
    out
}

// ---------- piani ----------
#[derive(Clone)]
struct Plane {
    data: Vec<u8>, // 1bpp MSB-first, righe allineate a byte
    width_px: u32,
    height_px: u32,
}

impl Plane {
    fn clone_data(&self) -> Plane {
        Plane { data: self.data.clone(), width_px: self.width_px, height_px: self.height_px }
    }
    fn empty(width_px: u32, height_px: u32) -> Self {
        Plane {
            data: vec![0u8; (((width_px + 7) / 8) * height_px) as usize],
            width_px,
            height_px,
        }
    }
    fn row_len(&self) -> usize {
        ((self.width_px + 7) / 8) as usize
    }
    fn set(&mut self, x: u32, y: u32) {
        let rl = self.row_len();
        self.data[y as usize * rl + (x as usize) / 8] |= 0x80 >> (x % 8);
    }
}

// ---------- stream PJL+PCL (dal template PIE "OKI WT Series Production") ----------
fn build_stream(prof: &Profile, planes: &[Plane], with_pjl: bool, plane_header: i32) -> Vec<u8> {
    let mut s: Vec<u8> = Vec::with_capacity(4 * 1024 * 1024);

    // ===== PJL (opzionale: lo spooler Windows a volte lo corrompe) =====
    if with_pjl {
    s.extend_from_slice(
        format!(
            "{}%-12345X@PJL JOB NAME=\"{}\"\r\n@PJL OKIAUXJOBINFO DATA=\"DocumentName={0}\"\r\n@PJL OKIAUXJOBINFO DATA=\"MultiPage=1\"\r\n@PJL OKIAUXJOBINFO DATA=\"ApplicationName=WTC  ProRIP\"\r\n@PJL RDYMSG DISPLAY = \"{0} -WTC  ProRIP\"\r\n",
            ESC as char, prof.job_title
        )
        .as_bytes(),
    );
    s.extend_from_slice(b"@PJL SET OKIPAPERSIZECHECK=DISABLE\r\n"); // tray 4 -> DISABLE
    s.extend_from_slice(b"@PJL SET OKIUNITOFMEASURE=MILLIMETER\r\n");
    s.extend_from_slice(format!("@PJL SET OKIXDIMENSIONMILLIMETER={}\r\n", prof.xmm).as_bytes());
    s.extend_from_slice(format!("@PJL SET OKIYDIMENSIONMILLIMETER={}\r\n", prof.ymm).as_bytes());
    s.extend_from_slice(
        b"@PJL SET MEDIASRCMANUAL=2\r\n@PJL SET MEDIASRCINTRAY1=4\r\n@PJL SET MEDIASRCINTRAY2=1\r\n@PJL SET MEDIASRCINTRAY3=5\r\n@PJL SET MEDIASRCINTRAY4=6\r\n@PJL SET MEDIASRCINTRAY5=20\r\n@PJL SET MEDIASRCINTRAY6=21\r\n@PJL SET MEDIASRCINTRAY7=22\r\n",
    );
    s.extend_from_slice(b"@PJL SET QTY=1\r\n@PJL SET COPIES=1\r\n");
    s.extend_from_slice(b"@PJL SET RESOLUTION=V1200\r\n"); // 600x1200 -> V1200
    s.extend_from_slice(
        b"@PJL SET LPARM:PCL OKIPRINTMARGIN=INCH1D6\r\n@PJL SET OKICOLORSCREEN=FINESCREEN\r\n@PJL SET OKICOLORBKGENERATEMODE=IMAGE1GRAPHIC1\r\n@PJL SET LPARM:PCL OKITRUEBLACK=OFF\r\n",
    );
    s.extend_from_slice(b"@PJL ENTER LANGUAGE=PCL\r\n");
    }

    // ===== PCL init (letterale dal PIE) =====
    s.extend_from_slice(b"\x1BE\x0D");
    s.extend_from_slice(b"\x1B&l4H"); // MultiPurpose tray
    s.extend_from_slice(format!("\x1B&n{}W{}", prof.media_name.len(), prof.media_name).as_bytes());
    s.extend_from_slice(b"\x1B&l0S"); // simplex
    s.extend_from_slice(b"\x1B&l101A"); // custom size via PJL mm
    s.extend_from_slice(b"\x1B/l1O"); // short edge first
    s.extend_from_slice(b"\x1B&l0E\x1B&a0L\x1B&a0H\x1B&a0V");
    s.extend_from_slice(b"\x1B*p50Y");
    s.extend_from_slice(b"\x1B*r0F");
    s.extend_from_slice(b"\x1B*r1A");
    s.extend_from_slice(b"\x1B*b0Y");
    s.extend_from_slice(b"\x1B%1A");
    s.extend_from_slice(format!("\x1B*r-{}U", plane_header).as_bytes()); // N planes
    s.extend_from_slice(b"\x1B&u1200D");
    s.extend_from_slice(b"\x1B/t1200R"); // variante slash per 1200dpi
    s.extend_from_slice(b"\x1B*b2M"); // TIFF packbits

    // ===== RIGHE INTERLACCIATE (dal capture DF reale!) =====
    // Per ogni riga: 4 blocchi compressi = piani [Cyan, White, Magenta, Yellow]
    // ESC*b<len:06digits>V = continua, ESC*b<len>W = chiude la riga (4° piano)
    // NESSUN selettore di piano: la posizione dei blocchi È il piano.
    // NESSUN salto righe: ogni riga viene trasmessa sempre (anche vuota).
    let rl = planes[0].row_len();
    for y in 0..planes[0].height_px {
        let last = planes.len() - 1;
        for (i, plane) in planes.iter().enumerate() {
            let row = &plane.data[(y as usize) * rl..(y as usize + 1) * rl];
            let comp = packbits(row);
            let letter = if i < last { 'V' } else { 'W' };
            s.extend_from_slice(format!("\x1B*b{:06}{}", comp.len(), letter).as_bytes());
            s.extend_from_slice(&comp);
        }
    }

    // ===== stop =====
    s.extend_from_slice(b"\x1B*rC\x1BE\x1B%-12345X");
    s
}

// ---------- silhouette bianca con choke (erosione box separabile) ----------
// mode "silhouette": bianco = tutta la silhouette (alpha > 10)
// mode "explicit":   bianco = SOLO dove hai dipinto bianco (alpha>10 && pixel quasi bianco)
fn white_mask_from_alpha(img: &DynamicImage, choke_px: u32, explicit: bool) -> Vec<bool> {
    let (w, h) = img.dimensions();
    let (w, h) = (w as usize, h as usize);
    let rgba = img.to_rgba8();
    let mut mask: Vec<bool> = rgba
        .pixels()
        .map(|p: &Rgba<u8>| {
            let [r, g, b, a] = p.0;
            if a <= 10 {
                return false;
            }
            if explicit {
                // pixel "bianco dipinto": tutti i canali alti
                r > 240 && g > 240 && b > 240
            } else {
                true
            }
        })
        .collect();
    if choke_px == 0 {
        return mask;
    }
    let r = choke_px as isize;
    let mut tmp = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut ok = true;
            for dx in -r..=r {
                let xx = x as isize + dx;
                if xx < 0 || xx >= w as isize || !mask[y * w + xx as usize] {
                    ok = false;
                    break;
                }
            }
            tmp[y * w + x] = ok;
        }
    }
    let mut eroded = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut ok = true;
            for dy in -r..=r {
                let yy = y as isize + dy;
                if yy < 0 || yy >= h as isize || !tmp[yy as usize * w + x] {
                    ok = false;
                    break;
                }
            }
            eroded[y * w + x] = ok;
        }
    }
    eroded
}

// ---------- ICC: sRGB -> CMYK (Cadlink), intent 14 come nel psqx ----------
fn rgb_to_cmyk_icc(img: &DynamicImage) -> Vec<[u8; 4]> {
    use lcms2::{Intent, PixelFormat, Profile, Transform};
    let src_prof =
        Profile::new_file("profiles/CADlink Unified RGB.icm").expect("profilo RGB Cadlink mancante");
    let dst_prof =
        Profile::new_file("profiles/CadlinkWideCMYK.icm").expect("profilo CMYK Cadlink mancante");
    let transform = Transform::new(
        &src_prof,
        PixelFormat::RGB_8,
        &dst_prof,
        PixelFormat::CMYK_8,
        Intent::PreserveKPlaneRelativeColorimetric,
    )
    .expect("trasformazione ICC fallita");
    let src: Vec<[u8; 3]> = img
        .to_rgba8()
        .pixels()
        .map(|p: &Rgba<u8>| [p.0[0], p.0[1], p.0[2]])
        .collect();
    let mut dst: Vec<[u8; 4]> = vec![[0u8; 4]; src.len()];
    transform.transform_pixels(&src, &mut dst);
    dst
}

// ---------- halftone Floyd-Steinberg 1bpp ----------
fn halftone_plane(values: &[f32], w: u32, h: u32) -> Vec<u8> {
    let mut buf = values.to_vec();
    let mut out = vec![0u8; (((w + 7) / 8) * h) as usize];
    let row_len = ((w + 7) / 8) as usize;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let old = buf[i];
            let new = if old >= 0.5 { 1.0 } else { 0.0 };
            if new > 0.5 {
                out[(y as usize) * row_len + (x as usize) / 8] |= 0x80 >> (x % 8);
            }
            let err = old - new;
            let mut push = |xx: i64, yy: i64, e: f32| {
                if xx >= 0 && xx < w as i64 && yy >= 0 && yy < h as i64 {
                    let j = (yy as u32 * w + xx as u32) as usize;
                    buf[j] += e;
                }
            };
            push(x as i64 + 1, y as i64, err * 7.0 / 16.0);
            push(x as i64 - 1, y as i64 + 1, err * 3.0 / 16.0);
            push(x as i64, y as i64 + 1, err * 5.0 / 16.0);
            push(x as i64 + 1, y as i64 + 1, err * 1.0 / 16.0);
        }
    }
    out
}

// ---------- PCL pagina: init + stop (dal template PIE) ----------
fn pcl_page_start(s: &mut Vec<u8>, prof: &Profile) {
    s.push(0x1B);
    s.extend_from_slice(b"E\x0D");
    s.extend_from_slice(b"\x1B&l4H"); // MultiPurpose tray
    s.extend_from_slice(format!("\x1B&n{}W{}", prof.media_name.len(), prof.media_name).as_bytes());
    s.extend_from_slice(b"\x1B&l0S\x1B&l101A\x1B/l1O");
    s.extend_from_slice(b"\x1B&l0E\x1B&a0L\x1B&a0H\x1B&a0V");
    s.extend_from_slice(b"\x1B*p50Y\x1B*r0F\x1B*r1A\x1B*b0Y\x1B%1A");
    s.extend_from_slice(b"\x1B*r-4U"); // 4 planes
    s.extend_from_slice(b"\x1B&u1200D\x1B/t1200R\x1B*b2M");
}

fn pcl_page_end(s: &mut Vec<u8>) {
    s.extend_from_slice(b"\x1B*rC\x1BE\x0C"); // end raster, reset, form feed
}

// ---------- TEST BARS: 4 bande piene, una per piano ----------
fn make_bars(width_px: u32, height_px: u32) -> [Plane; 4] {
    let mut planes = Vec::new();
    for p in 0..4u32 {
        let mut plane = Plane::empty(width_px, height_px);
        let band_h = height_px / 4;
        let y0 = p * band_h + band_h / 4;
        let y1 = p * band_h + (band_h * 3) / 4;
        for y in y0..y1 {
            for x in 0..width_px {
                plane.set(x, y);
            }
        }
        planes.push(plane);
    }
    let mut it = planes.into_iter();
    [
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()).unwrap_or("help") {
        "bars" => {
            let w = (7.93f64 * 600.0).round() as u32; // 4758
            let h = (11.36f64 * 1200.0).round() as u32; // 13632
            let prof = a4_profile("OKI-DECAL BARS TEST");
            let planes = make_bars(w, h);
            let no_pjl = std::env::args().any(|a| a == "--no-pjl");
            let stream = build_stream(&prof, &planes, !no_pjl, 4);
            fs::write("bars.prn", &stream).expect("scrittura bars.prn");
            println!(
                "bars.prn scritto ({} byte). Bande ALTO->BASSO = piano 1(Cyan),2(White),3(Magenta),4(Yellow)",
                stream.len()
            );
            println!(
                "Invio: powershell -File test/Send-RAW.ps -PrinterName \"NOMESTAMPANTE\" -File bars.prn"
            );
        }
        "bars2" => {
            // TEST DEFINITIVO: 4 pagine, UN piano per pagina, banda centrata
            let w = (7.93f64 * 600.0).round() as u32;
            let h = (11.36f64 * 1200.0).round() as u32;
            let prof = a4_profile("OKI-DECAL BARS2");
            let mut s: Vec<u8> = Vec::new();
            for p in 1..=4u32 {
                let mut plane = Plane::empty(w, h);
                let y0 = h / 3;
                let y1 = (h * 2) / 3;
                for y in y0..y1 {
                    for x in 0..w {
                        plane.set(x, y);
                    }
                }
                pcl_page_start(&mut s, &prof);
                s.extend_from_slice(format!("\x1B*b{}6V", p).as_bytes());
                let rl = plane.row_len();
                let mut zero_rows: u32 = 0;
                for y in 0..plane.height_px {
                    let row = &plane.data[(y as usize) * rl..(y as usize + 1) * rl];
                    if row.iter().all(|&b| b == 0) {
                        zero_rows += 1;
                        if zero_rows == 32767 {
                            s.extend_from_slice(format!("\x1B*b{}Y", zero_rows).as_bytes());
                            zero_rows = 0;
                        }
                        continue;
                    }
                    if zero_rows > 0 {
                        s.extend_from_slice(format!("\x1B*b{}Y", zero_rows).as_bytes());
                        zero_rows = 0;
                    }
                    let comp = packbits(row);
                    s.extend_from_slice(format!("\x1B*b{}W", comp.len()).as_bytes());
                    s.extend_from_slice(&comp);
                }
                if zero_rows > 0 {
                    s.extend_from_slice(format!("\x1B*b{}Y", zero_rows).as_bytes());
                }
                pcl_page_end(&mut s);
            }
            fs::write("bars2.prn", &s).expect("scrittura bars2.prn");
            println!(
                "bars2.prn scritto ({} byte). 4 pagine: pagina N = piano N (comando ESC*b<N>6V), banda al centro",
                s.len()
            );
            println!(
                "Invio: powershell -File test/Send-RAW.ps1 -PrinterName \"OKI C711WT\" -File bars2.prn"
            );
        }
        "barsdw1" => {
            // V1: 4 slot, bianco DUPLICATO al posto del magento: [C, W, W, Y]
            let w = (7.93f64 * 600.0).round() as u32;
            let h = (11.36f64 * 1200.0).round() as u32;
            let prof = a4_profile("DW1");
            let mut planes = make_bars(w, h);
            // sostituisci il piano 3 (magenta) con una COPIA del piano 2 (bianco)
            planes[2] = Plane { data: planes[1].data.clone(), width_px: w, height_px: h };
            let s = build_stream(&prof, &planes, false, 4);
            fs::write("barsdw1.prn", &s).expect("write");
            println!("barsdw1.prn: 4 bande [C, W, W-copia, Y] - se la banda 3 esce BIANCA (non magenta) = double-white ON");
        }
        "barsdw2" => {
            // V2: 5 blocchi per riga con *r-5U: [C, W, W2, M, Y]
            let w = (7.93f64 * 600.0).round() as u32;
            let h = (11.36f64 * 1200.0).round() as u32;
            let prof = a4_profile("DW2");
            let four = make_bars(w, h);
            let mut planes = vec![four[0].clone_data(), four[1].clone_data(), four[1].clone_data(), four[2].clone_data(), four[3].clone_data()];
            let _ = &mut planes;
            let refs: Vec<Plane> = planes.drain(..).collect();
            let s = build_stream(&prof, &refs, false, 5);
            fs::write("barsdw2.prn", &s).expect("write");
            println!("barsdw2.prn: 5 bande [C, W, W2, M, Y] con *r-5U - se la banda 3 esce BIANCA = double-white a 5 piani");
        }
        "decal" => {
            let input = args.get(2).expect("uso: oki-decal decal <immagine.png> [--white=explicit|silhouette]");
            let explicit = args.iter().any(|a| a.contains("white=explicit"));
            let no_pjl = std::env::args().any(|a| a == "--no-pjl");
            let mirror = args.iter().any(|a| a == "--mirror");
            let media = args
                .iter()
                .find(|a| a.starts_with("--media="))
                .map(|a| media_code(&a[8..]));
            let mut prof = a4_profile("OKI-DECAL");
            if let Some(m) = media {
                prof.media_name = m;
            }
            let img = image::open(input).expect("immagine non leggibile");
            let pw = (7.93f64 * 600.0).round() as u32;
            let ph = (11.36f64 * 1200.0).round() as u32;
            let (iw, ih) = img.dimensions();

            println!(
                "Silhouette bianca (choke 3px, modalita' {})...",
                if explicit { "ESPLICITA (bianco dipinto)" } else { "SILHOUETTE (sotto-bianco)" }
            );
            // scala immagine per entrare nella pagina (margini ~0.35in)
            let max_w = pw - (2.0f64 * 0.35 * 600.0).round() as u32;
            let max_h = ph - (2.0f64 * 0.35 * 1200.0).round() as u32;
            let scale = (max_w as f64 / iw as f64).min(max_h as f64 / ih as f64).min(1.0);
            let dw = ((iw as f64) * scale).round() as u32;
            let dh = ((ih as f64) * scale).round() as u32;
            let resized = img.resize_exact(dw, dh, image::imageops::FilterType::Lanczos3);
            // mask calcolata SULL'IMMAGINE RIDIMENSIONATA (stessa dimensione dei piani)
            let mask = white_mask_from_alpha(&resized, 3, explicit);

            println!("ICC -> CMYK (Cadlink)...");
            let cmyk = rgb_to_cmyk_icc(&resized);

            println!("Curve + halftone...");
            let mut cv = vec![0f32; (dw * dh) as usize];
            let mut mv = vec![0f32; (dw * dh) as usize];
            let mut yv = vec![0f32; (dw * dh) as usize];
            for (i, px) in cmyk.iter().enumerate() {
                let (c, m, y, k) = (px[0], px[1], px[2], px[3]);
                let (cf, mf, yf, kf) = (
                    c as f32 / 255.0,
                    m as f32 / 255.0,
                    y as f32 / 255.0,
                    k as f32 / 255.0,
                );
                if explicit && mask[i] {
                    // KNOCKOUT: dove l'utente ha dipinto bianco, niente CMY sopra
                    cv[i] = 0.0;
                    mv[i] = 0.0;
                    yv[i] = 0.0;
                } else {
                    // niente toner nero: K piegato in CMY
                    cv[i] = apply_curve((cf + kf * (1.0 - cf)).min(1.0), &CURVE_C);
                    mv[i] = apply_curve((mf + kf * (1.0 - mf)).min(1.0), &CURVE_M);
                    yv[i] = apply_curve((yf + kf * (1.0 - yf)).min(1.0), &CURVE_Y);
                }
            }
            let cp = halftone_plane(&cv, dw, dh);
            let mp = halftone_plane(&mv, dw, dh);
            let yp = halftone_plane(&yv, dw, dh);

            // componi piani a pagina intera, immagine centrata
            let x0 = ((pw - dw) / 2) as usize;
            let y0 = ((ph - dh) / 2) as usize;
            let mut c_full = Plane::empty(pw, ph);
            let mut m_full = Plane::empty(pw, ph);
            let mut y_full = Plane::empty(pw, ph);
            let mut w_full = Plane::empty(pw, ph);
            for yy in 0..dh {
                for xx in 0..dw {
                    // con --mirror specchia orizzontalmente (fonte letta al contrario)
                    let sx = if mirror { dw - 1 - xx } else { xx };
                    // indice nel piano 1bpp compresso (righe allineate al byte)
                    let sp = (yy as usize) * (((dw as usize) + 7) / 8) + (sx as usize) / 8;
                    let bit = 0x80 >> (sx % 8);
                    let (px, py) = (x0 + xx as usize, y0 + yy as usize);
                    if cp[sp] & bit != 0 {
                        c_full.set(px as u32, py as u32);
                    }
                    if mp[sp] & bit != 0 {
                        m_full.set(px as u32, py as u32);
                    }
                    if yp[sp] & bit != 0 {
                        y_full.set(px as u32, py as u32);
                    }
                    if mask[(yy as usize) * dw as usize + sx as usize] {
                        w_full.set(px as u32, py as u32);
                    }
                }
            }

            let planes = [c_full, w_full, m_full, y_full];
            let no_pjl = std::env::args().any(|a| a == "--no-pjl");
            let stream = build_stream(&prof, &planes, !no_pjl, 4);
            fs::write("decal.prn", &stream).expect("scrittura decal.prn");
            println!("decal.prn scritto ({} byte)", stream.len());
            println!(
                "Invio: powershell -File test/Send-RAW.ps -PrinterName \"NOMESTAMPANTE\" -File decal.prn"
            );
        }
        _ => {
            println!("oki-decal - decal printer OKI C711WT");
            println!("  oki-decal bars [--no-pjl]     -> bars.prn (4 bande test)");
            println!("  oki-decal decal <file.png> [opzioni] -> decal.prn");
            println!("Opzioni decal:");
            println!("  --white=explicit   bianco solo dove dipinto bianco (default: silhouette completa)");
            println!("  --mirror           specchia orizzontalmente");
            println!("  --media=NOME       plain|letterhead|transparency|labels|bond|cardstock|glossy (default glossy)");
            println!("  --pjl              include header PJL (default: NO, lo spooler lo corrompe)");
            println!("Stampa: powershell -File test/Send-RAW.ps1 -PrinterName \"OKI C711WT\" -File oki-decal/decal.prn");
        }
    }
}
