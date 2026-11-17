//! Icona nella tray, notifiche e istanza singola. Win32 diretto: niente crate in più e la notifica
//! esce col nome dell'app (un balloon NIF_INFO su Windows 10/11 diventa un toast).
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::CreateBitmap;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const TITLE: &str = "Ovhelm";
const CB: u32 = WM_APP + 1;
const ID_OPEN: usize = 1;
const ID_QUIT: usize = 2;

static TRAY: AtomicIsize = AtomicIsize::new(0);
static MAIN: AtomicIsize = AtomicIsize::new(0);
static READY: AtomicBool = AtomicBool::new(false);

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
fn copy(dst: &mut [u16], s: &str) {
    let n = dst.len() - 1;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(n)) {
        *d = c;
    }
}

/// Icona: quadrato blu (#0050D7) con il prompt ">_" bianco, come `ui::mark`.
/// Antialias con 4x4 campioni per pixel.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    // distanza punto-segmento, per disegnare tratti spessi
    let seg = |px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32| -> f32 {
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        ((px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)).sqrt()
    };
    let inside = |x: f32, y: f32| -> [f32; 4] {
        let (h, r) = (s * 0.48, s * 0.22);
        let (qx, qy) = ((x - s / 2.0).abs() - (h - r), (y - s / 2.0).abs() - (h - r));
        if (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() > r {
            return [0.0; 4];
        }
        let w = s * 0.05; // mezzo spessore del tratto
        let p = |fx: f32, fy: f32| (fx * s, fy * s);
        let ((a1, b1), (a2, b2), (a3, b3), (a4, b4), (a5, b5)) = (p(0.24, 0.30), p(0.44, 0.50), p(0.24, 0.70), p(0.52, 0.70), p(0.78, 0.70));
        let d = seg(x, y, a1, b1, a2, b2).min(seg(x, y, a2, b2, a3, b3)).min(seg(x, y, a4, b4, a5, b5));
        if d <= w {
            return [1.0, 1.0, 1.0, 1.0];
        }
        [0.0, 0x50 as f32 / 255.0, 0xD7 as f32 / 255.0, 1.0]
    };
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..4 {
                for sx in 0..4 {
                    let c = inside(x as f32 + (sx as f32 + 0.5) / 4.0, y as f32 + (sy as f32 + 0.5) / 4.0);
                    for k in 0..3 {
                        acc[k] += c[k] * c[3];
                    }
                    acc[3] += c[3];
                }
            }
            let i = ((y * size + x) * 4) as usize;
            for k in 0..3 {
                out[i + k] = (acc[k] / acc[3].max(1e-6) * 255.0) as u8;
            }
            out[i + 3] = (acc[3] / 16.0 * 255.0) as u8;
        }
    }
    out
}

/// Se c'è già un'istanza la porta davanti e ritorna false.
pub fn single_instance() -> bool {
    unsafe {
        let name = w("ovhelm-single-instance");
        CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let t = w(TITLE);
            let h = FindWindowW(std::ptr::null(), t.as_ptr());
            if !h.is_null() {
                ShowWindow(h, SW_SHOW);
                if IsIconic(h) != 0 {
                    ShowWindow(h, SW_RESTORE);
                }
                SetForegroundWindow(h);
            }
            return false;
        }
    }
    true
}

pub fn set_main(hwnd: isize) {
    MAIN.store(hwnd, Ordering::Relaxed);
    // senza barra di Windows la finestra verrebbe con gli angoli vivi: chiede a DWM quelli arrotondati
    let pref: i32 = windows_sys::Win32::Graphics::Dwm::DWMWCP_ROUND;
    unsafe {
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd as HWND,
            windows_sys::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &pref as *const i32 as _,
            4,
        );
    }
}
/// "Effetti di animazione" di Windows: se sono spenti il banner resta fermo. Letto una volta.
pub fn animations_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let mut on: i32 = 1;
        let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut on as *mut i32 as _, 0) };
        ok == 0 || on != 0
    })
}

pub fn ready() -> bool {
    READY.load(Ordering::Relaxed)
}
pub fn hide_main() {
    let h = MAIN.load(Ordering::Relaxed) as HWND;
    if !h.is_null() {
        unsafe { ShowWindow(h, SW_HIDE) };
    }
}
pub fn show_main() {
    let h = MAIN.load(Ordering::Relaxed) as HWND;
    if !h.is_null() {
        unsafe {
            ShowWindow(h, SW_SHOW);
            if IsIconic(h) != 0 {
                ShowWindow(h, SW_RESTORE);
            }
            SetForegroundWindow(h);
        }
    }
}

fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut n: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    n.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n
}

/// Notifica di Windows dall'icona della tray. `warn` = icona di avviso.
pub fn notify(title: &str, text: &str, warn: bool) {
    let h = TRAY.load(Ordering::Relaxed) as HWND;
    if h.is_null() {
        return;
    }
    let mut n = nid(h);
    n.uFlags = NIF_INFO;
    copy(&mut n.szInfoTitle, title);
    copy(&mut n.szInfo, text);
    n.dwInfoFlags = if warn { NIIF_WARNING } else { NIIF_INFO };
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &n) };
}

fn quit() -> ! {
    let h = TRAY.load(Ordering::Relaxed) as HWND;
    if !h.is_null() {
        let n = nid(h);
        unsafe { Shell_NotifyIconW(NIM_DELETE, &n) };
    }
    std::process::exit(0)
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == CB {
        match (lp & 0xFFFF) as u32 {
            WM_LBUTTONUP | WM_LBUTTONDBLCLK => show_main(),
            WM_RBUTTONUP | WM_CONTEXTMENU => {
                let m = CreatePopupMenu();
                let (open, q) = (w(crate::i18n::tr("tray.apri")), w(crate::i18n::tr("tray.esci")));
                AppendMenuW(m, MF_STRING, ID_OPEN, open.as_ptr());
                AppendMenuW(m, MF_STRING, ID_QUIT, q.as_ptr());
                let mut p = POINT { x: 0, y: 0 };
                GetCursorPos(&mut p);
                SetForegroundWindow(h); // senza, il menu non si chiude cliccando fuori
                let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON, p.x, p.y, 0, h, std::ptr::null());
                DestroyMenu(m);
                match cmd as usize {
                    ID_OPEN => show_main(),
                    ID_QUIT => quit(),
                    _ => {}
                }
            }
            _ => {}
        }
        return 0;
    }
    DefWindowProcW(h, msg, wp, lp)
}

/// Thread della tray con la sua coda messaggi.
pub fn start() {
    std::thread::spawn(|| unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let cls = w("ovhelm-tray");
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(proc);
        wc.hInstance = inst;
        wc.lpszClassName = cls.as_ptr();
        RegisterClassW(&wc);
        let h = CreateWindowExW(0, cls.as_ptr(), cls.as_ptr(), 0, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), inst, std::ptr::null());
        if h.is_null() {
            return;
        }
        let size = 32;
        let mut bgra = icon_rgba(size);
        for p in bgra.chunks_mut(4) {
            p.swap(0, 2);
        }
        let mask = vec![0u8; (size * size / 8) as usize];
        let mut ii = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: CreateBitmap(size as i32, size as i32, 1, 1, mask.as_ptr() as _),
            hbmColor: CreateBitmap(size as i32, size as i32, 1, 32, bgra.as_ptr() as _),
        };
        let icon = CreateIconIndirect(&mut ii);
        let mut n = nid(h);
        n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        n.uCallbackMessage = CB;
        n.hIcon = icon;
        copy(&mut n.szTip, TITLE);
        if Shell_NotifyIconW(NIM_ADD, &n) == 0 {
            return;
        }
        TRAY.store(h as isize, Ordering::Relaxed);
        READY.store(true, Ordering::Relaxed);
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    });
}
