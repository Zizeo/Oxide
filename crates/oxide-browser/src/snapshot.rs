//! Freezes the live page into an egui texture so egui menus can be drawn on
//! top of it.
//!
//! The page is a native child `wry` WebView, which the OS always paints above
//! the egui surface. A menu drawn by egui would therefore be buried under the
//! page. Hiding the page makes the menu visible but blanks the viewport, which
//! is not what a browser should do. Instead we copy the page's pixels once,
//! hide the WebView, and paint the copy where the page was: the menu and the
//! page are then visible at the same time.
//!
//! The copy is taken with `PrintWindow(.., PW_RENDERFULLCONTENT)`, which is the
//! only capture path that returns the WebView2 content. `BitBlt` from the
//! screen DC returns the window background instead, because WebView2 renders
//! through DirectComposition.

use egui::{ColorImage, TextureHandle};

/// A frozen copy of the page, plus the GPU texture it was uploaded to.
///
/// The texture is created lazily on the first frame the copy is painted and
/// kept for as long as the copy lives, so the pixels are uploaded once per
/// menu open rather than once per frame.
pub struct PageSnapshot {
    pub image: ColorImage,
    pub texture: Option<TextureHandle>,
}

/// Captures the visible page child of `parent` (the main window handle).
///
/// Returns `None` when there is no visible page or the capture fails.
#[cfg(windows)]
pub fn capture_page(parent: isize) -> Option<ColorImage> {
    use windows::core::w;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, IsWindowVisible};

    let parent = HWND(parent as *mut core::ffi::c_void);
    // Every tab owns a `WRY_WEBVIEW` child; only the active one is visible.
    let mut child = unsafe { FindWindowExW(parent, None, w!("WRY_WEBVIEW"), None) }.ok()?;
    while !child.is_invalid() {
        if unsafe { IsWindowVisible(child).as_bool() }
            && let Some(image) = capture_window(child)
        {
            return Some(image);
        }
        child = unsafe { FindWindowExW(parent, child, w!("WRY_WEBVIEW"), None) }.ok()?;
    }
    None
}

#[cfg(not(windows))]
pub fn capture_page(_parent: isize) -> Option<ColorImage> {
    None
}

#[cfg(windows)]
fn capture_window(hwnd: windows::Win32::Foundation::HWND) -> Option<ColorImage> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
        ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, PW_RENDERFULLCONTENT};

    unsafe {
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect).ok()?;
        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;
        if w <= 0 || h <= 0 {
            return None;
        }

        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);
        let printed = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();

        let mut image = None;
        if printed {
            let mut bi = BITMAPINFO::default();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = -h; // top-down rows
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB.0;

            let mut buf = vec![0u8; (w * h * 4) as usize];
            let lines = GetDIBits(
                mem,
                bmp,
                0,
                h as u32,
                Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                &mut bi,
                DIB_RGB_COLORS,
            );
            if lines != 0 {
                image = Some(bgra_to_image(&buf, w, h));
            }
        }

        SelectObject(mem, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        image
    }
}

/// Converts a top-down 32-bit BGRA buffer (as GDI hands it back) into an egui
/// image. Alpha is forced opaque: `PrintWindow` leaves the alpha channel
/// undefined, and egui would otherwise blend the page away.
#[cfg_attr(not(windows), allow(dead_code))]
fn bgra_to_image(buf: &[u8], w: i32, h: i32) -> ColorImage {
    let mut pixels = Vec::with_capacity((w * h) as usize);
    for i in (0..buf.len()).step_by(4) {
        pixels.push(egui::Color32::from_rgba_unmultiplied(
            buf[i + 2],
            buf[i + 1],
            buf[i],
            255,
        ));
    }
    ColorImage {
        size: [w as usize, h as usize],
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_pixels_are_reordered_to_rgba() {
        // One pixel: B=10, G=20, R=30, A=40 (alpha ignored).
        let image = bgra_to_image(&[10, 20, 30, 40], 1, 1);
        assert_eq!(image.size, [1, 1]);
        assert_eq!(
            image.pixels[0],
            egui::Color32::from_rgba_unmultiplied(30, 20, 10, 255)
        );
    }

    #[test]
    fn bgra_conversion_keeps_row_order() {
        // Two pixels, top row first: red then blue.
        let image = bgra_to_image(&[0, 0, 255, 0, 255, 0, 0, 0], 2, 1);
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(255, 0, 0));
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(0, 0, 255));
    }
}
