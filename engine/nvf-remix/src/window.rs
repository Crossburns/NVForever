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

/// The runtime's menu window (its input sink), as an isize; 0 until found.
static SINK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
/// The game window's own procedure, called after ours.
static GAME_PROC: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
/// The runtime's menu is open (Alt+X toggles it) [documented: the runtime's
/// rtx_options.h remixMenuKeyBinds, default Alt+X].
static MENU_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the runtime's menu (Alt+X) is open, as the keys pressed say.
pub fn menu_open() -> bool {
    MENU_OPEN.load(std::sync::atomic::Ordering::Relaxed)
}

/// The game window's procedure while its raw input is shared: every WM_INPUT goes to the
/// runtime's menu window first, then to the game's own handler; Alt+X flips the menu flag.
unsafe extern "system" fn share_proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::UI::Input::{GetRawInputData, RAWINPUT, RAWINPUTHEADER, RID_INPUT, RIM_TYPEKEYBOARD};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_MENU};
    use windows_sys::Win32::UI::WindowsAndMessaging::{CallWindowProcW, SendMessageW, WM_INPUT};
    if m == WM_INPUT {
        // SAFETY: the raw input handle is valid while this message is handled; the buffer
        // is sized by the first call.
        unsafe {
            let mut size = 0u32;
            let header = std::mem::size_of::<RAWINPUTHEADER>() as u32;
            if GetRawInputData(l as _, RID_INPUT, std::ptr::null_mut(), &mut size, header) == 0 && size as usize >= std::mem::size_of::<RAWINPUTHEADER>() {
                let mut buf = vec![0u8; size as usize];
                if GetRawInputData(l as _, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, header) == size {
                    let ri = &*(buf.as_ptr() as *const RAWINPUT);
                    if ri.header.dwType == RIM_TYPEKEYBOARD {
                        let k = ri.data.keyboard;
                        // A key going down (RI_KEY_BREAK, bit 0, clear): X with Alt held.
                        if k.VKey == 0x58 && k.Flags & 1 == 0 && GetKeyState(i32::from(VK_MENU)) < 0 {
                            MENU_OPEN.fetch_xor(true, Ordering::Relaxed);
                        }
                    }
                }
            }
            let sink = SINK.load(Ordering::Relaxed);
            if sink != 0 {
                SendMessageW(sink as HWND, m, w, l);
            }
        }
    }
    // While the menu is open it has the keyboard and mouse to itself: input messages skip the
    // game's handler and get Windows' default handling (Alt+F4 still closes the window), so a
    // click in the menu is not also a shot in the game.
    if MENU_OPEN.load(Ordering::Relaxed) && is_input(m) {
        use windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW;
        // SAFETY: the default procedure for this window's message.
        return unsafe { DefWindowProcW(h, m, w, l) };
    }
    let prev = GAME_PROC.load(Ordering::Relaxed);
    // SAFETY: the procedure this replaced, for every message.
    unsafe { CallWindowProcW(std::mem::transmute::<isize, windows_sys::Win32::UI::WindowsAndMessaging::WNDPROC>(prev), h, m, w, l) }
}

/// Keyboard, mouse and raw input messages.
fn is_input(m: u32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_CHAR, WM_INPUT, WM_KEYDOWN, WM_KEYUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDBLCLK};
    matches!(m, WM_INPUT | WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR | WM_MOUSEWHEEL | WM_MOUSEHWHEEL) || (WM_MOUSEMOVE..=WM_XBUTTONDBLCLK).contains(&m)
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

    /// Share the mouse's and keyboard's raw input between the game and the runtime's
    /// menu window ("RemixGuiInputSink"), once that window exists: true when done. The
    /// runtime registers the process's raw mouse and keyboard to its window (the keyboard
    /// without legacy key messages), and a process has one target per device, so the game
    /// got neither mouse looks nor keys; registered back to the game's window, with key
    /// messages, and every WM_INPUT handed on to the menu window, both get them
    /// [documented: the runtime's rtx_overlay_window.cpp windowThreadMain and its WM_INPUT
    /// handling; observed 2026-11-09, no movement with the menu window, no menu without].
    pub fn share_raw_input(&self) -> bool {
        use std::sync::atomic::Ordering;
        use windows_sys::Win32::UI::Input::{RegisterRawInputDevices, RAWINPUTDEVICE, RIDEV_INPUTSINK};
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetWindowThreadProcessId, SetWindowLongPtrW, GWLP_WNDPROC};
        if SINK.load(Ordering::Relaxed) != 0 {
            return true;
        }
        let class = super::wide("RemixGuiInputSink");
        // SAFETY: window lookups and registrations on this process's own windows.
        unsafe {
            let me = windows_sys::Win32::System::Threading::GetCurrentProcessId();
            let mut sink: HWND = std::ptr::null_mut();
            loop {
                sink = FindWindowExW(std::ptr::null_mut(), sink, class.as_ptr(), std::ptr::null());
                if sink.is_null() {
                    return false;
                }
                let mut pid = 0u32;
                GetWindowThreadProcessId(sink, &mut pid);
                if pid == me {
                    break;
                }
            }
            let devices = [
                RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: RIDEV_INPUTSINK, hwndTarget: self.parent },
                RAWINPUTDEVICE { usUsagePage: 1, usUsage: 6, dwFlags: RIDEV_INPUTSINK, hwndTarget: self.parent },
            ];
            if RegisterRawInputDevices(devices.as_ptr(), 2, std::mem::size_of::<RAWINPUTDEVICE>() as u32) == 0 {
                return false;
            }
            let prev = SetWindowLongPtrW(self.parent, GWLP_WNDPROC, share_proc as *const () as usize as isize);
            GAME_PROC.store(prev, Ordering::Relaxed);
            SINK.store(sink as isize, Ordering::Relaxed);
        }
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
