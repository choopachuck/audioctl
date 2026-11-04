//! Turns a device glyph into the HICON the notification area displays.

use std::mem::size_of;

use audioctl_core::icon::{render_bgra, IconRequest};
use windows::core::{w, PCWSTR};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, FindWindowW, HICON, ICONINFO, SM_CXSMICON};

pub fn tray_px() -> i32 {
    unsafe {
        let dpi = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok().map(|hwnd| GetDpiForWindow(hwnd)).filter(|dpi| *dpi >= 96).unwrap_or(96);
        GetSystemMetricsForDpi(SM_CXSMICON, dpi).max(16)
    }
}

pub fn render(req: &IconRequest) -> Option<HICON> {
    let pixels = render_bgra(req)?;
    let premul: Vec<u32> = pixels.into_iter().map(premultiply).collect();
    hicon(&premul, req.size as i32)
}

fn premultiply(p: u32) -> u32 {
    let a = (p >> 24) & 0xFF;
    let scale = |c: u32| (c * a + 127) / 255;
    a << 24 | scale((p >> 16) & 0xFF) << 16 | scale((p >> 8) & 0xFF) << 8 | scale(p & 0xFF)
}

fn hicon(pixels: &[u32], size: i32) -> Option<HICON> {
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return None;
        }
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
        let Some(dib) = CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok() else {
            let _ = DeleteDC(hdc);
            return None;
        };
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast(), pixels.len());
        let mask_bits = vec![0u8; ((size + 15) / 16 * 2 * size) as usize];
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
        let info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: dib };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(HGDIOBJ(mask.0));
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(hdc);
        icon
    }
}
