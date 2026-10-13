// oki-decal: decal printer per OKI C711WT (toner swap fisico: White,Yellow,Magenta,Cyan)
// Genera stream PJL+PCL con 4 piani (3ColorWhite: Cyan,White,Magenta,Yellow)
// Profili ICC Cadlink in profiles/, ricetta in profiles/underprint_live.json

use image::{DynamicImage, GenericImageView, Rgba};
use std::fs;

const ESC: u8 = 0x1B;

// ---------- profilo di stampa ----------
pub struct Profile {
    xmm: i32,
    ymm: i32,
    media_name: &'static str,   // es. "dGlossy"
    job_title: String,
}

// mappa nome -> comando PCL media type (dal PDX: MediaType)
pub fn media_code(name: &str) -> &'static str {
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

pub fn a4_profile(job_title: &str) -> Profile {
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
pub const CURVE_C: [f32; 13] = [0.0000, 0.0532, 0.0911, 0.1646, 0.2408, 0.3032, 0.3715, 0.4631, 0.5873, 0.6669, 0.7599, 0.8116, 1.0000];
pub const CURVE_M: [f32; 13] = [0.0000, 0.0728, 0.0963, 0.1561, 0.2343, 0.3093, 0.3724, 0.4163, 0.4751, 0.5817, 0.6901, 0.7658, 1.0000];
pub const CURVE_Y: [f32; 13] = [0.0000, 0.0590, 0.1033, 0.1812, 0.2673, 0.3276, 0.4056, 0.4777, 0.5447, 0.6405, 0.7584, 0.8006, 1.0000];

pub fn apply_curve(v: f32, curve: &[f32; 13]) -> f32 {
    let x = v.clamp(0.0, 1.0) * 12.0;
    let i = (x as usize).min(11);
    let f = x - i as f32;
    curve[i] + (curve[i + 1] - curve[i]) * f
}

// ---------- TIFF/PackBits (PCL compression mode 2) ----------
pub fn packbits(row: &[u8]) -> Vec<u8> {
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
pub struct Plane {
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
pub fn build_stream(prof: &Profile, planes: &[Plane], with_pjl: bool, plane_header: i32) -> Vec<u8> {
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
    if with_pjl {
        // 600 x 1200 (V1200 via PJL)
        s.extend_from_slice(b"\x1B*r-4U");
        s.extend_from_slice(b"\x1B&u1200D\x1B/t1200R");
    } else {
        // 600 x 600 (modalita' nativa senza PJL)
        s.extend_from_slice(b"\x1B*r-4U");
        s.extend_from_slice(b"\x1B&u600D\x1B*t600R");
    }
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
pub fn white_mask_from_alpha(img: &DynamicImage, choke_px: u32, explicit: bool) -> Vec<bool> {
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
pub fn rgb_to_cmyk_icc(img: &DynamicImage) -> Vec<[u8; 4]> {
    use lcms2::{Intent, PixelFormat, Profile, Transform};

    // ricerca profili: cwd, cartella exe, ../.. (target/release)
    fn find_profile(name: &str) -> std::path::PathBuf {
        let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let mut candidates = vec![std::path::PathBuf::from(format!("profiles/{name}"))];
        if let Some(d) = &exe {
            candidates.push(d.join("profiles").join(name));
            candidates.push(d.join("..").join("..").join("profiles").join(name));
        }
        candidates
            .into_iter()
            .find(|p| p.exists())
            .unwrap_or_else(|| panic!("profilo {name} non trovato in profiles/"))
    }

    let src_prof = Profile::new_file(find_profile("CADlink Unified RGB.icm")).expect("profilo RGB Cadlink mancante");
    let dst_prof = Profile::new_file(find_profile("CadlinkWideCMYK.icm")).expect("profilo CMYK Cadlink mancante");
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
        .map(|p: &Rgba<u8>| {
            let [r, g, b, a] = p.0;
            // COMPOSIZIONE SU BIANCO: i pixel trasparenti spesso nascondono RGB nero;
            // senza questo verrebbero stampati come nero ricco (bug fondo nero)
            let af = a as f32 / 255.0;
            let r2 = (r as f32 * af + 255.0 * (1.0 - af)) as u8;
            let g2 = (g as f32 * af + 255.0 * (1.0 - af)) as u8;
            let b2 = (b as f32 * af + 255.0 * (1.0 - af)) as u8;
            [r2, g2, b2]
        })
        .collect();
    let mut dst: Vec<[u8; 4]> = vec![[0u8; 4]; src.len()];
    transform.transform_pixels(&src, &mut dst);
    dst
}

// ---------- halftone cluster 45° (screen tipografico per colori piatti) ----------
fn halftone_dots45(values: &[f32], w: u32, h: u32) -> Vec<u8> {
    // screen a 45 gradi: griglia ruotata di celle 8x8 su coordinate (u,v) = ruota(x,y)
    // soglia del cluster = matrice bayer su (u mod 8, v mod 8)
    const BAYER8: [[u16; 8]; 8] = [
        [0, 32, 8, 40, 2, 34, 10, 42],
        [48, 16, 56, 24, 50, 18, 58, 26],
        [12, 44, 4, 36, 14, 46, 6, 38],
        [60, 28, 52, 20, 62, 30, 54, 22],
        [3, 35, 11, 43, 1, 33, 9, 41],
        [51, 19, 59, 27, 49, 17, 57, 25],
        [15, 47, 7, 39, 13, 45, 5, 37],
        [63, 31, 55, 23, 61, 29, 53, 21],
    ];
    let mut out = vec![0u8; (((w + 7) / 8) * h) as usize];
    let row_len = ((w + 7) / 8) as usize;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let old = values[i].clamp(0.0, 1.0);
            // coordinate ruotate 45 gradi: celle ~1/150 pollice a 600dpi
            let u = ((x as i64 + y as i64) / 4).rem_euclid(8) as usize;
            let v = ((x as i64 - y as i64 + 8 * 4096) / 4).rem_euclid(8) as usize;
            // +1: soglia minima 1/64 — i residui ~0 non generano griglie di puntini
            let thr = (BAYER8[u][v] as f32 + 1.0) / 64.0;
            let on = old > thr;
            if on {
                out[(y as usize) * row_len + (x as usize) / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    out
}

// ---------- halftone Floyd-Steinberg 1bpp ----------
pub fn halftone_plane(values: &[f32], w: u32, h: u32) -> Vec<u8> {
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
pub fn pcl_page_start(s: &mut Vec<u8>, prof: &Profile) {
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

pub fn pcl_page_end(s: &mut Vec<u8>) {
    s.extend_from_slice(b"\x1B*rC\x1BE\x0C"); // end raster, reset, form feed
}

// ---------- TEST BARS: 4 bande piene, una per piano ----------
pub fn make_bars(width_px: u32, height_px: u32) -> [Plane; 4] {
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


// ================= API PUBBLICA (CLI + GUI) =================

#[derive(Clone)]
pub struct DecalOptions {
    pub explicit_white: bool,
    pub mirror: bool,
    pub media: String, // plain|letterhead|transparency|labels|bond|cardstock|glossy
    pub with_pjl: bool,
    pub width_mm: f32, // 0 = adatta alla pagina (centrata)
    pub x_mm: f32,     // offset da sinistra (mm)
    pub y_mm: f32,     // offset dall'alto (mm)
    pub dots45: bool,  // true = screen cluster 45° (colori piatti), false = diffusion (foto)
}

impl Default for DecalOptions {
    fn default() -> Self {
        DecalOptions {
            explicit_white: false,
            mirror: false,
            media: "glossy".into(),
            with_pjl: false,
            width_mm: 0.0,
            x_mm: 20.0,
            y_mm: 20.0,
            dots45: true,
        }
    }
}

pub fn decal_stream(input: &str, opts: &DecalOptions) -> Result<Vec<u8>, String> {
    let img = image::open(input).map_err(|e| format!("immagine non leggibile: {e}"))?;
    let mut prof = a4_profile("OKI-DECAL");
    prof.media_name = media_code(&opts.media);
    // senza PJL: 600x600 (geometria corretta); con PJL: 600x1200 (V1200)
    let dpi_y = if opts.with_pjl { 1200.0f64 } else { 600.0 };
    let pw = (7.93f64 * 600.0).round() as u32;
    let ph = (11.36f64 * dpi_y).round() as u32;
    let (iw, ih) = img.dimensions();

    // dimensione di stampa: width_mm se impostato, altrimenti adatta alla pagina
    let m2dotx = 600.0 / 25.4;
    let m2doty = dpi_y / 25.4;
    let max_w = pw - (2.0f64 * 0.25 * 600.0).round() as u32;
    let max_h = ph - (2.0f64 * 0.25 * 1200.0).round() as u32;
    let scale = if opts.width_mm > 0.0 {
        (opts.width_mm as f64 * m2dotx) / iw as f64
    } else {
        (max_w as f64 / iw as f64).min(max_h as f64 / ih as f64).min(1.0)
    };
    let dw = ((iw as f64) * scale).round().max(1.0) as u32;
    let dh = ((ih as f64) * scale).round().max(1.0) as u32;
    let resized = img.resize_exact(dw, dh, image::imageops::FilterType::Lanczos3);

    let mask = white_mask_from_alpha(&resized, 3, opts.explicit_white);
    let cmyk = rgb_to_cmyk_icc(&resized);

    let mut cv = vec![0f32; (dw * dh) as usize];
    let mut mv = vec![0f32; (dw * dh) as usize];
    let mut yv = vec![0f32; (dw * dh) as usize];
    for (i, px) in cmyk.iter().enumerate() {
        let (cf, mf, yf, kf) = (
            px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0, px[3] as f32 / 255.0,
        );
        if opts.explicit_white && mask[i] {
            cv[i] = 0.0; mv[i] = 0.0; yv[i] = 0.0; // knockout
        } else {
            // niente toner nero: K piegato in CMY + soglia anti-griglia
            // (il bianco carta dall'ICC esce con residui ~1%: lo screen 45°
            //  li trasformerebbe in una griglia di puntini — taglio sotto il 3%)
            cv[i] = apply_curve((cf + kf * (1.0 - cf)).min(1.0), &CURVE_C);
            mv[i] = apply_curve((mf + kf * (1.0 - mf)).min(1.0), &CURVE_M);
            yv[i] = apply_curve((yf + kf * (1.0 - yf)).min(1.0), &CURVE_Y);
            if cv[i] < 0.03 { cv[i] = 0.0; }
            if mv[i] < 0.03 { mv[i] = 0.0; }
            if yv[i] < 0.03 { yv[i] = 0.0; }
        }
    }
    let cp = if opts.dots45 {
        halftone_dots45(&cv, dw, dh)
    } else {
        halftone_plane(&cv, dw, dh)
    };
    let mp = if opts.dots45 {
        halftone_dots45(&mv, dw, dh)
    } else {
        halftone_plane(&mv, dw, dh)
    };
    let yp = if opts.dots45 {
        halftone_dots45(&yv, dw, dh)
    } else {
        halftone_plane(&yv, dw, dh)
    };

    // posizionamento: offset mm dall'alto-sinistra (trascinabili nella GUI)
    let mut x0 = (opts.x_mm as f64 * m2dotx).round() as usize;
    let mut y0 = (opts.y_mm as f64 * m2doty).round() as usize;
    if x0 + dw as usize > pw as usize { x0 = (pw as usize).saturating_sub(dw as usize); }
    if y0 + dh as usize > ph as usize { y0 = (ph as usize).saturating_sub(dh as usize); }
    let mut c_full = Plane::empty(pw, ph);
    let mut m_full = Plane::empty(pw, ph);
    let mut y_full = Plane::empty(pw, ph);
    let mut w_full = Plane::empty(pw, ph);
    for yy in 0..dh {
        for xx in 0..dw {
            let sx = if opts.mirror { dw - 1 - xx } else { xx };
            let sp = (yy as usize) * (((dw as usize) + 7) / 8) + (sx as usize) / 8;
            let bit = 0x80 >> (sx % 8);
            let (px, py) = (x0 + xx as usize, y0 + yy as usize);
            if cp[sp] & bit != 0 { c_full.set(px as u32, py as u32); }
            if mp[sp] & bit != 0 { m_full.set(px as u32, py as u32); }
            if yp[sp] & bit != 0 { y_full.set(px as u32, py as u32); }
            if mask[(yy as usize) * dw as usize + sx as usize] { w_full.set(px as u32, py as u32); }
        }
    }
    let planes = [c_full, w_full, m_full, y_full];
    Ok(build_stream(&prof, &planes, opts.with_pjl, 4))
}

pub fn bars_stream(with_pjl: bool) -> Vec<u8> {
    let w = (7.93f64 * 600.0).round() as u32;
    let h = (11.36f64 * 1200.0).round() as u32;
    let prof = a4_profile("OKI-DECAL BARS");
    let planes = make_bars(w, h);
    build_stream(&prof, &planes, with_pjl, 4)
}

/// quadrato di calibrazione 100x100mm + righello: verifica geometrica della stampa
pub fn calib_stream() -> Vec<u8> {
    let pw = (7.93f64 * 600.0).round() as u32;
    let ph = (11.36f64 * 600.0).round() as u32;
    let prof = a4_profile("OKI CALIB 100mm");
    let side = (100.0f64 * 600.0 / 25.4).round() as u32; // 100mm = 2362 px @600
    let x0 = (20.0f64 * 600.0 / 25.4).round() as usize;
    let y0 = (20.0f64 * 600.0 / 25.4).round() as usize;
    let mut c = Plane::empty(pw, ph);
    let mut m = Plane::empty(pw, ph);
    for y in y0..y0 + side as usize {
        for x in x0..x0 + side as usize {
            // meta' sinistra ciano, meta' destra magenta: se e' tutta un colore = DPI X/Y sbagliati
            if x < x0 + (side as usize) / 2 {
                c.set(x as u32, y as u32);
            } else {
                m.set(x as u32, y as u32);
            }
        }
    }
    // righello: tacche ogni 25mm lungo il bordo alto (0.5x3mm ciano)
    for mm in 0..=250u32 {
        let x = (20.0f64 * 600.0 / 25.4).round() as usize + (mm as f64 * 600.0 / 25.4).round() as usize;
        for yy in (5.0f64 * 600.0 / 25.4).round() as usize..((5.0f64 + 3.0) * 600.0 / 25.4).round() as usize {
            for xx in x..x + 3 {
                c.set(xx as u32, yy as u32);
            }
        }
    }
    build_stream(&prof, &[c, Plane::empty(pw, ph), m, Plane::empty(pw, ph)], false, 4)
}
