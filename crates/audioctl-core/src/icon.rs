//! Tray and HTTP icons. Glyphs come from Segoe Fluent Icons (Windows 11) or Segoe MDL2 Assets.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct IconRequest {
    pub glyph: char,
    pub color: (u8, u8, u8),
    pub file: Option<std::path::PathBuf>,
    pub muted: bool,
    pub badge: Option<u32>,
    pub size: u32,
}

pub fn parse_color(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim().trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255);
    (p(0), p(2), p(4))
}

/// Straight-alpha pixels, packed `A<<24 | R<<16 | G<<8 | B`.
pub fn render_bgra(req: &IconRequest) -> Option<Vec<u32>> {
    let size = req.size as i32;
    if !(8..=512).contains(&size) {
        return None;
    }
    let mut pixels = if let Some(path) = req.file.as_deref().filter(|p| p.is_file()) {
        load_file(path, size).unwrap_or_else(|| glyph_pixels(req.glyph, req.color, size))
    } else {
        glyph_pixels(req.glyph, req.color, size)
    };
    if pixels.len() != (size * size) as usize {
        return None;
    }
    if req.muted {
        slash(&mut pixels, size);
    }
    if let Some(volume) = req.badge {
        badge(&mut pixels, size, volume.min(100));
    }
    Some(pixels)
}

pub fn render_png(req: &IconRequest) -> Option<Vec<u8>> {
    let pixels = render_bgra(req)?;
    let size = req.size;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for p in pixels {
        rgba.extend_from_slice(&[((p >> 16) & 0xFF) as u8, ((p >> 8) & 0xFF) as u8, (p & 0xFF) as u8, ((p >> 24) & 0xFF) as u8]);
    }
    let image = image::RgbaImage::from_raw(size, size, rgba)?;
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(buf.into_inner())
}

fn glyph_pixels(ch: char, color: (u8, u8, u8), size: i32) -> Vec<u32> {
    #[cfg(windows)]
    {
        if let Some(raw) = draw_glyph(ch, size) {
            return colorize(&raw, color);
        }
    }
    let _ = (ch, color);
    fallback_disc(color, size)
}

fn colorize(src: &[u32], color: (u8, u8, u8)) -> Vec<u32> {
    src.iter()
        .map(|p| {
            let a = luminance(*p).min(255) as u8;
            pack(color.0, color.1, color.2, a)
        })
        .collect()
}

fn fallback_disc(color: (u8, u8, u8), size: i32) -> Vec<u32> {
    let mut pixels = vec![0u32; (size * size) as usize];
    let r = size as f32 * 0.32;
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            if dx * dx + dy * dy <= r * r {
                pixels[(y * size + x) as usize] = pack(color.0, color.1, color.2, 255);
            }
        }
    }
    pixels
}

fn load_file(path: &Path, size: i32) -> Option<Vec<u32>> {
    let image = image::ImageReader::open(path).ok()?.decode().ok()?.resize(size as u32, size as u32, image::imageops::FilterType::Triangle);
    let rgba = image.to_rgba8();
    Some(rgba.pixels().map(|p| pack(p[0], p[1], p[2], p[3])).collect())
}

/// A red slash, so a muted device is obvious on top of either a glyph or a custom file.
pub(crate) fn slash(pixels: &mut [u32], size: i32) {
    let color = (0xFF, 0x45, 0x3A, 230u8);
    for i in 0..size {
        let x = size - 1 - i;
        let y = i;
        for dy in -1..=1 {
            paint(pixels, size, x, y + dy, color.0, color.1, color.2, color.3);
        }
    }
}

fn badge(pixels: &mut [u32], size: i32, volume: u32) {
    let text = volume.to_string();
    let scale = if size >= 32 { 2 } else { 1 };
    let gap = scale;
    let digit_w = 3 * scale;
    let width = text.len() as i32 * (digit_w + gap) - gap;
    let height = 5 * scale;
    if width + 2 > size || height + 2 > size {
        return;
    }
    let origin_x = size - 1 - width;
    let origin_y = size - 1 - height;
    for (i, ch) in text.chars().enumerate() {
        let digit = ch.to_digit(10).unwrap_or(0) as usize;
        let ox = origin_x + i as i32 * (digit_w + gap);
        blit_digit(pixels, size, ox, origin_y, digit, scale, (0, 0, 0, 200));
    }
    for (i, ch) in text.chars().enumerate() {
        let digit = ch.to_digit(10).unwrap_or(0) as usize;
        let ox = origin_x + i as i32 * (digit_w + gap);
        blit_digit(pixels, size, ox, origin_y - scale, digit, scale, (255, 255, 255, 240));
    }
}

fn blit_digit(pixels: &mut [u32], size: i32, x: i32, y: i32, digit: usize, scale: i32, color: (u8, u8, u8, u8)) {
    let rows = DIGITS[digit.min(9)];
    for (row, bits) in rows.iter().enumerate() {
        for col in 0..3 {
            if bits & (1 << (2 - col)) == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    paint(pixels, size, x + col * scale + sx, y + row as i32 * scale + sy, color.0, color.1, color.2, color.3);
                }
            }
        }
    }
}

const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

fn paint(pixels: &mut [u32], size: i32, x: i32, y: i32, sr: u8, sg: u8, sb: u8, sa: u8) {
    if x < 0 || y < 0 || x >= size || y >= size || sa == 0 {
        return;
    }
    let dst = &mut pixels[(y * size + x) as usize];
    let (dr, dg, db, da) = unpack(*dst);
    let sa_f = sa as u32;
    let inv = 255 - sa_f;
    let mix = |s: u8, d: u8| ((s as u32 * sa_f + d as u32 * inv + 127) / 255) as u8;
    let a = (sa_f + (da as u32 * inv + 127) / 255).min(255) as u8;
    *dst = pack(mix(sr, dr), mix(sg, dg), mix(sb, db), a);
}

fn pack(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

fn unpack(p: u32) -> (u8, u8, u8, u8) {
    (((p >> 16) & 0xFF) as u8, ((p >> 8) & 0xFF) as u8, (p & 0xFF) as u8, ((p >> 24) & 0xFF) as u8)
}

fn luminance(p: u32) -> u32 {
    (p & 0xFF).max((p >> 8) & 0xFF).max((p >> 16) & 0xFF)
}

#[cfg(windows)]
fn draw_glyph(ch: char, size: i32) -> Option<Vec<u32>> {
    use std::mem::size_of;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush, SelectObject,
        SetBkMode, SetTextColor, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
        DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FW_NORMAL, HDC, HGDIOBJ,
        OUT_TT_PRECIS, TRANSPARENT,
    };

    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let drawn = draw_on(hdc, ch, size);
        let _ = DeleteDC(hdc);
        return drawn;

        unsafe fn draw_on(hdc: HDC, ch: char, size: i32) -> Option<Vec<u32>> {
            let font_name = symbol_font();
            let face: Vec<u16> = font_name.encode_utf16().chain(std::iter::once(0)).collect();
            let height = (size * 72 / 100).max(1);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size,
                    biHeight: -size,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let count = (size * size) as usize;
            std::ptr::write_bytes(bits, 0, count * 4);
            let old_bmp = SelectObject(hdc, HGDIOBJ(dib.0));
            let hfont = CreateFontW(
                height,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_TT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                0,
                PCWSTR(face.as_ptr()),
            );
            if hfont.is_invalid() {
                SelectObject(hdc, old_bmp);
                let _ = DeleteObject(HGDIOBJ(dib.0));
                return None;
            }
            let old_font = SelectObject(hdc, HGDIOBJ(hfont.0));
            SetTextColor(hdc, COLORREF(0x00FF_FFFF));
            SetBkMode(hdc, TRANSPARENT);
            let mut wide: Vec<u16> = ch.to_string().encode_utf16().collect();
            let mut rc = RECT { left: 0, top: 0, right: size, bottom: size };
            DrawTextW(hdc, &mut wide, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            let _ = GdiFlush();
            let pixels = std::slice::from_raw_parts(bits as *const u32, count).to_vec();
            SelectObject(hdc, old_font);
            SelectObject(hdc, old_bmp);
            let _ = DeleteObject(HGDIOBJ(hfont.0));
            let _ = DeleteObject(HGDIOBJ(dib.0));
            Some(pixels)
        }
    }
}

#[cfg(windows)]
fn symbol_font() -> &'static str {
    use std::sync::OnceLock;
    static FONT: OnceLock<&'static str> = OnceLock::new();
    FONT.get_or_init(|| {
        if face_installed("Segoe Fluent Icons") {
            "Segoe Fluent Icons"
        } else {
            "Segoe MDL2 Assets"
        }
    })
}

#[cfg(windows)]
fn face_installed(name: &str) -> bool {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, EnumFontFamiliesExW, DEFAULT_CHARSET, LOGFONTW, TEXTMETRICW};

    unsafe extern "system" fn callback(_: *const LOGFONTW, _: *const TEXTMETRICW, _: u32, data: LPARAM) -> i32 {
        unsafe { *(data.0 as *mut bool) = true };
        0
    }

    let mut face = [0u16; 32];
    for (i, c) in name.encode_utf16().take(31).enumerate() {
        face[i] = c;
    }
    let mut found = false;
    unsafe {
        let hdc = CreateCompatibleDC(None);
        let mut lf = LOGFONTW { lfCharSet: DEFAULT_CHARSET, lfFaceName: face, ..Default::default() };
        EnumFontFamiliesExW(hdc, &mut lf, Some(callback), LPARAM(&mut found as *mut bool as isize), 0);
        let _ = DeleteDC(hdc);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color("#64D2FF"), (0x64, 0xD2, 0xFF));
        assert_eq!(parse_color("30d158"), (0x30, 0xD1, 0x58));
    }

    #[test]
    fn slash_paints_red() {
        let size = 16i32;
        let mut pixels = vec![0u32; (size * size) as usize];
        slash(&mut pixels, size);
        assert!(pixels.iter().any(|p| ((p >> 16) & 0xFF) > 200 && (p & 0xFF) < 80));
    }

    #[test]
    fn png_has_the_signature() {
        let png = render_png(&IconRequest {
            glyph: '\u{E7F5}',
            color: (0x64, 0xD2, 0xFF),
            file: None,
            muted: true,
            badge: Some(42),
            size: 32,
        })
        .unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']);
    }

    #[test]
    fn custom_file_is_used() {
        let path = std::env::temp_dir().join("audioctl-icon-test.png");
        let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
        image.save(&path).unwrap();
        let pixels = render_bgra(&IconRequest {
            glyph: '\u{E7F5}',
            color: (255, 0, 0),
            file: Some(path),
            muted: false,
            badge: None,
            size: 8,
        })
        .unwrap();
        let (r, g, b, _) = unpack(pixels[0]);
        assert!(r < 40 && g < 50 && b < 60, "file pixels should survive, got {r},{g},{b}");
    }
}
