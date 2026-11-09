//! The window Remix presents into: a child of NVForever's own window, covering its client
//! area, disabled so the mouse and keyboard reach the parent. Remix owns this child's
//! swapchain while Bevy keeps its own on the parent (a window has one presenter).

use std::ffi::c_void;
use std::path::Path;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, SetDllDirectoryW};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, MoveWindow, RegisterClassW, SetForegroundWindow, CS_OWNDC, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS, WS_DISABLED, WS_VISIBLE,
};

/// Put `dir` on the DLL search path (the runtime's own DLLs beside it).
pub fn add_dll_dir(dir: &Path) {
    let w = super::wide(&dir.to_string_lossy());
    // SAFETY: a NUL-terminated path.
    unsafe { SetDllDirectoryW(w.as_ptr()) };
}

/// The child window.
pub struct Child {
    pub hwnd: HWND,
    parent: HWND,
    size: (i32, i32),
}

unsafe extern "system" fn proc_(h: HWND, m: u32, w: usize, l: isize) -> isize {
    // SAFETY: the default procedure for this window's messages.
    unsafe { DefWindowProcW(h, m, w, l) }
}

impl Child {
    /// A child covering `parent`'s client area.
    pub fn new(parent: *mut c_void) -> Option<Child> {
        let parent = parent as HWND;
        let class = super::wide("NVForeverRemixOutput");
        // SAFETY: a static class description; registering twice fails harmlessly.
        unsafe {
            let hinst = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW {
                style: CS_OWNDC,
                lpfnWndProc: Some(proc_),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            RegisterClassW(&wc);
            let (w, h) = client_size(parent);
            let title = super::wide("Remix");
            let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_CHILD | WS_VISIBLE | WS_DISABLED | WS_CLIPSIBLINGS, 0, 0, w, h, parent, std::ptr::null_mut(), hinst, std::ptr::null());
            if hwnd.is_null() {
                return None;
            }
            Some(Child { hwnd, parent, size: (w, h) })
        }
    }

    /// Follow the parent's size; true when it changed.
    pub fn follow(&mut self) -> bool {
        let s = client_size(self.parent);
        if s == self.size || s.0 <= 0 || s.1 <= 0 {
            return false;
        }
        self.size = s;
        // SAFETY: our own window.
        unsafe { MoveWindow(self.hwnd, 0, 0, s.0, s.1, 1) };
        true
    }

    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    /// Bring the parent to the front (a test aid, before [`Child::capture_bmp`]).
    pub fn raise(&self) {
        // SAFETY: a window handle.
        unsafe { SetForegroundWindow(self.parent) };
    }

    /// The child's pixels as the screen shows them, as a 32-bit BMP file (a test aid: the
    /// window must be in front).
    pub fn capture_bmp(&self) -> Vec<u8> {
        let (w, h) = self.size;
        // SAFETY: GDI calls on our own window, objects freed after.
        unsafe {
            // The window's own picture (its client area, as composed with the Remix
            // child's swapchain), never the screen: what lies over the window (another
            // program) is not in it [observed 2026-11-09: a screen copy took the owner's
            // other windows].
            let screen = GetDC(self.parent);
            let mem = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, w, h);
            SelectObject(mem, bmp);
            // PW_CLIENTONLY | PW_RENDERFULLCONTENT.
            windows_sys::Win32::Storage::Xps::PrintWindow(self.parent, mem, 1 | 2);
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = h;
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            let mut buf = vec![0u8; (w * h * 4).max(0) as usize];
            GetDIBits(mem, bmp, 0, h as u32, buf.as_mut_ptr().cast(), &mut bi, DIB_RGB_COLORS);
            DeleteObject(bmp);
            DeleteDC(mem);
            ReleaseDC(self.parent, screen);
            let mut f = Vec::with_capacity(54 + buf.len());
            f.extend_from_slice(b"BM");
            f.extend_from_slice(&(54 + buf.len() as u32).to_le_bytes());
            f.extend_from_slice(&[0; 4]);
            f.extend_from_slice(&54u32.to_le_bytes());
            f.extend_from_slice(&40u32.to_le_bytes());
            f.extend_from_slice(&w.to_le_bytes());
            f.extend_from_slice(&h.to_le_bytes());
            f.extend_from_slice(&1u16.to_le_bytes());
            f.extend_from_slice(&32u16.to_le_bytes());
            f.extend_from_slice(&[0; 24]);
            f.extend_from_slice(&buf);
            f
        }
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        // SAFETY: our own window.
        unsafe { DestroyWindow(self.hwnd) };
    }
}

fn client_size(h: HWND) -> (i32, i32) {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: a valid out pointer.
    unsafe { GetClientRect(h, &mut r) };
    (r.right - r.left, r.bottom - r.top)
}
