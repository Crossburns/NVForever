//! `cargo run -p nvf-remix --example smoke -- <path to Remix d3d9.dll> [frames]`: a window,
//! the runtime loaded in-process, a lit quad and a floor path-traced for some frames.

#[cfg(windows)]
fn main() {
    use nvf_remix::ffi::{HardcodedVertex, Transform, CATEGORY_WORLD_UI};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    let _ = CATEGORY_WORLD_UI;
    let mut args = std::env::args().skip(1);
    let dll = std::path::PathBuf::from(args.next().expect("the Remix d3d9.dll path"));
    let frames: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(300);

    unsafe extern "system" fn wndproc(h: windows_sys::Win32::Foundation::HWND, m: u32, w: usize, l: isize) -> isize {
        unsafe { DefWindowProcW(h, m, w, l) }
    }
    let class = nvf_remix::wide("NvfRemixSmoke");
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW { style: 0, lpfnWndProc: Some(wndproc), cbClsExtra: 0, cbWndExtra: 0, hInstance: hinst, hIcon: std::ptr::null_mut(), hCursor: std::ptr::null_mut(), hbrBackground: std::ptr::null_mut(), lpszMenuName: std::ptr::null(), lpszClassName: class.as_ptr() };
        RegisterClassW(&wc);
        let title = nvf_remix::wide("NVForever Remix smoke");
        CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 100, 100, 1280, 720, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null())
    };
    assert!(!hwnd.is_null());

    let t0 = std::time::Instant::now();
    let mut remix = match nvf_remix::Remix::load(&dll) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("load: {e}");
            std::process::exit(2);
        }
    };
    println!("loaded in {:.2} s", t0.elapsed().as_secs_f32());
    if let Err(e) = remix.startup(hwnd.cast()) {
        eprintln!("{e}");
        std::process::exit(3);
    }
    println!("started in {:.2} s", t0.elapsed().as_secs_f32());

    // A 4x4 white/red checker texture.
    let mut px = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            px.extend_from_slice(if (x + y) % 2 == 0 { &[230, 230, 230, 255] } else { &[200, 40, 40, 255] });
        }
    }
    let tex_hash = 0x4E56_4600_0000_0001;
    remix.create_texture(&nvf_remix::TextureData { hash: tex_hash, width: 4, height: 4, mips: 1, format: nvf_remix::ffi::FORMAT_R8G8B8A8_SRGB, data: &px, vk_format: 0, encoding: 0 }).expect("texture");
    let mat = remix
        .create_material(&nvf_remix::MaterialDesc { hash: 0x4E56_4D00_0000_0001, albedo: Some(tex_hash), albedo_constant: [1.0; 3], opacity: 1.0, roughness: 0.6, ..Default::default() })
        .expect("material");
    let v = |p: [f32; 3], n: [f32; 3], uv: [f32; 2]| HardcodedVertex { position: p, normal: n, texcoord: uv, color: 0xFFFF_FFFF, _pad: [0; 7] };
    // Y up, Z toward the camera. A floor and an upright quad.
    let floor = [v([-5.0, 0.0, -5.0], [0.0, 1.0, 0.0], [0.0, 0.0]), v([5.0, 0.0, -5.0], [0.0, 1.0, 0.0], [4.0, 0.0]), v([5.0, 0.0, 5.0], [0.0, 1.0, 0.0], [4.0, 4.0]), v([-5.0, 0.0, 5.0], [0.0, 1.0, 0.0], [0.0, 4.0])];
    let wall = [v([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0]), v([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0]), v([1.0, 2.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0]), v([-1.0, 2.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0])];
    let idx = [0u32, 2, 1, 0, 3, 2];
    let m_floor = remix.create_mesh(0x4E56_0000_0000_0001, &floor, &idx, mat).expect("mesh");
    let m_wall = remix.create_mesh(0x4E56_0000_0000_0002, &wall, &[0, 1, 2, 0, 2, 3], mat).expect("mesh");
    let sun = remix.create_distant_light(0x4E56_1000_0000_0001, [-0.3, -1.0, -0.5], 0.5, [4.0, 3.8, 3.5]).unwrap_or_else(|e| { eprintln!("sun: {e}"); std::process::exit(4) });
    let lamp = remix.create_sphere_light(0x4E56_1000_0000_0002, [0.0, 1.0, 2.0], 0.2, [60.0, 50.0, 30.0]).unwrap_or_else(|e| { eprintln!("lamp: {e}"); std::process::exit(4) });

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    let start = std::time::Instant::now();
    let mut errors = 0;
    for f in 0..frames {
        unsafe {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let a = f as f32 * 0.01;
        let pos = [a.sin() * 6.0, 2.0, a.cos() * 6.0];
        let fwd = {
            let d = [-pos[0], 1.0 - pos[1], -pos[2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            [d[0] / l, d[1] / l, d[2] / l]
        };
        let right = {
            let r = [fwd[2], 0.0, -fwd[0]];
            let l = (r[0] * r[0] + r[2] * r[2]).sqrt();
            [-r[0] / l, 0.0, -r[2] / l]
        };
        let up = [right[1] * fwd[2] - right[2] * fwd[1], right[2] * fwd[0] - right[0] * fwd[2], right[0] * fwd[1] - right[1] * fwd[0]];
        let mut ok = remix.setup_camera(false, pos, fwd, up, right, 60.0, 16.0 / 9.0, 0.1, 1000.0).is_ok();
        ok &= remix.draw_instance(m_floor, &Transform::default(), 0, true).is_ok();
        ok &= remix.draw_instance(m_wall, &Transform::default(), 0, true).is_ok();
        ok &= remix.draw_light(sun).is_ok();
        ok &= remix.draw_light(lamp).is_ok();
        ok &= remix.present().is_ok();
        if !ok {
            errors += 1;
        }
    }
    // The window's pixels as the screen shows them, to a BMP beside the working folder.
    unsafe {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::Graphics::Gdi::*;
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(hwnd, &mut r);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        SelectObject(mem, bmp);
        BitBlt(mem, 0, 0, w, h, screen, r.left, r.top, SRCCOPY);
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = h;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(mem, bmp, 0, h as u32, buf.as_mut_ptr().cast(), &mut bi, DIB_RGB_COLORS);
        let mut f = Vec::new();
        let size = 54 + buf.len() as u32;
        f.extend_from_slice(b"BM");
        f.extend_from_slice(&size.to_le_bytes());
        f.extend_from_slice(&[0; 4]);
        f.extend_from_slice(&54u32.to_le_bytes());
        f.extend_from_slice(&40u32.to_le_bytes());
        f.extend_from_slice(&w.to_le_bytes());
        f.extend_from_slice(&h.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&32u16.to_le_bytes());
        f.extend_from_slice(&[0; 24]);
        f.extend_from_slice(&buf);
        let _ = std::fs::write("smoke.bmp", f);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
    let secs = start.elapsed().as_secs_f32();
    println!("{frames} frames in {secs:.2} s ({:.1} fps), {errors} frames with an error", frames as f32 / secs);
}

#[cfg(not(windows))]
fn main() {}
