//! The window Remix presents into: a child of NVForever's own window, covering its client
//! area, disabled so the mouse and keyboard reach the parent. Remix owns this child's
//! swapchain while Bevy keeps its own on the parent (a window has one presenter).

use std::ffi::c_void;
use std::path::Path;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, SetDllDirectoryW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, MoveWindow, RegisterClassW, CS_OWNDC, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS, WS_DISABLED, WS_VISIBLE,
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
