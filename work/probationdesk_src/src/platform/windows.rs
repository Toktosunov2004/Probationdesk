use super::{CursorData, ResultType};
use crate::{
    common::PORTABLE_APPNAME_RUNTIME_ENV_KEY,
    custom_server::*,
    ipc,
    privacy_mode::win_topmost_window::{self, WIN_TOPMOST_INJECTED_PROCESS_EXE},
};
use hbb_common::{
    allow_err,
    anyhow::anyhow,
    bail,
    config::{self, Config},
    libc::{c_int, wchar_t},
    log,
    message_proto::{DisplayInfo, Resolution, WindowsSession},
    sleep,
    sysinfo::{Pid, System},
    timeout, tokio,
};
use std::{
    collections::HashMap,
    ffi::{CString, OsString},
    fs,
    io::{self, prelude::*},
    mem,
    os::{
        raw::c_ulong,
        windows::{ffi::OsStringExt, process::CommandExt},
    },
    path::*,
    ptr::null_mut,
    sync::{atomic::Ordering, Arc, Mutex},
    time::{Duration, Instant},
};
use wallpaper;
#[cfg(not(debug_assertions))]
use winapi::um::libloaderapi::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_USER_DIRS};
use winapi::{
    ctypes::c_void,
    shared::{minwindef::*, ntdef::NULL, windef::*, winerror::*},
    um::{
        errhandlingapi::GetLastError,
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        libloaderapi::{
            GetProcAddress, LoadLibraryA, LoadLibraryExA, LOAD_LIBRARY_SEARCH_SYSTEM32,
        },
        minwinbase::STILL_ACTIVE,
        processthreadsapi::{
            GetCurrentProcess, GetCurrentProcessId, GetExitCodeProcess, OpenProcess,
            OpenProcessToken, ProcessIdToSessionId, PROCESS_INFORMATION, STARTUPINFOW,
        },
        securitybaseapi::{
            AllocateAndInitializeSid, DuplicateToken, EqualSid, FreeSid, GetTokenInformation,
        },
        shellapi::ShellExecuteW,
        sysinfoapi::{GetNativeSystemInfo, SYSTEM_INFO},
        winbase::*,
        wingdi::*,
        winnt::{
            SecurityImpersonation, TokenElevation, TokenGroups, TokenImpersonation, TokenType,
            DOMAIN_ALIAS_RID_ADMINS, ES_AWAYMODE_REQUIRED, ES_CONTINUOUS, ES_DISPLAY_REQUIRED,
            ES_SYSTEM_REQUIRED, HANDLE, PROCESS_ALL_ACCESS, PROCESS_QUERY_LIMITED_INFORMATION,
            PSID, SECURITY_BUILTIN_DOMAIN_RID, SECURITY_NT_AUTHORITY, SID_IDENTIFIER_AUTHORITY,
            TOKEN_ELEVATION, TOKEN_GROUPS, TOKEN_QUERY, TOKEN_TYPE,
        },
        winreg::HKEY_CURRENT_USER,
        winspool::{
            EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL,
            PRINTER_INFO_1W,
        },
        winuser::*,
    },
};
use windows::Win32::{
    Foundation::{CloseHandle as WinCloseHandle, HANDLE as WinHANDLE},
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    },
};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
};
use winreg::{enums::*, RegKey};

pub const FLUTTER_RUNNER_WIN32_WINDOW_CLASS: &'static str = "FLUTTER_RUNNER_WIN32_WINDOW"; // РіР»Р°РІРЅРѕРµ РѕРєРЅРѕ, РѕРєРЅРѕ СѓСЃС‚Р°РЅРѕРІРєРё
pub const EXPLORER_EXE: &'static str = "explorer.exe";
pub const SET_FOREGROUND_WINDOW: &'static str = "SET_FOREGROUND_WINDOW";

const REG_NAME_INSTALL_DESKTOPSHORTCUTS: &str = "DESKTOPSHORTCUTS";
const REG_NAME_INSTALL_STARTMENUSHORTCUTS: &str = "STARTMENUSHORTCUTS";
pub const REG_NAME_INSTALL_PRINTER: &str = "PRINTER";

pub fn get_focused_display(displays: Vec<DisplayInfo>) -> Option<usize> {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut rect: RECT = mem::zeroed();
        if GetWindowRect(hwnd, &mut rect as *mut RECT) == 0 {
            return None;
        }
        displays.iter().position(|display| {
            let center_x = rect.left + (rect.right - rect.left) / 2;
            let center_y = rect.top + (rect.bottom - rect.top) / 2;
            center_x >= display.x
                && center_x <= display.x + display.width
                && center_y >= display.y
                && center_y <= display.y + display.height
        })
    }
}

pub fn get_cursor_pos() -> Option<(i32, i32)> {
    unsafe {
        #[allow(invalid_value)]
        let mut out = mem::MaybeUninit::uninit().assume_init();
        if GetCursorPos(&mut out) == FALSE {
            return None;
        }
        return Some((out.x, out.y));
    }
}

pub fn reset_input_cache() {}

pub fn get_cursor() -> ResultType<Option<u64>> {
    unsafe {
        #[allow(invalid_value)]
        let mut ci: CURSORINFO = mem::MaybeUninit::uninit().assume_init();
        ci.cbSize = std::mem::size_of::<CURSORINFO>() as _;
        if crate::portable_service::client::get_cursor_info(&mut ci) == FALSE {
            return Err(io::Error::last_os_error().into());
        }
        if ci.flags & CURSOR_SHOWING == 0 {
            Ok(None)
        } else {
            Ok(Some(ci.hCursor as _))
        }
    }
}

struct IconInfo(ICONINFO);

impl IconInfo {
    fn new(icon: HICON) -> ResultType<Self> {
        unsafe {
            #[allow(invalid_value)]
            let mut ii = mem::MaybeUninit::uninit().assume_init();
            if GetIconInfo(icon, &mut ii) == FALSE {
                Err(io::Error::last_os_error().into())
            } else {
                let ii = Self(ii);
                if ii.0.hbmMask.is_null() {
                    bail!("Р СѓС‡РєР° Р±РёС‚РјР°РїР° РєСѓСЂСЃРѕСЂР° NULL");
                }
                return Ok(ii);
            }
        }
    }

    fn is_color(&self) -> bool {
        !self.0.hbmColor.is_null()
    }
}

impl Drop for IconInfo {
    fn drop(&mut self) {
        unsafe {
            if !self.0.hbmColor.is_null() {
                DeleteObject(self.0.hbmColor as _);
            }
            if !self.0.hbmMask.is_null() {
                DeleteObject(self.0.hbmMask as _);
            }
        }
    }
}

// https://github.com/TurboVNC/tightvnc/blob/a235bae328c12fd1c3aed6f3f034a37a6ffbbd22/vnc_winsrc/winvnc/vncEncoder.cpp
// https://github.com/TigerVNC/tigervnc/blob/master/win/rfb_win32/DeviceFrameBuffer.cxx
pub fn get_cursor_data(hcursor: u64) -> ResultType<CursorData> {
    unsafe {
        let mut ii = IconInfo::new(hcursor as _)?;
        let bm_mask = get_bitmap(ii.0.hbmMask)?;
        let mut width = bm_mask.bmWidth;
        let mut height = if ii.is_color() {
            bm_mask.bmHeight
        } else {
            bm_mask.bmHeight / 2
        };
        let cbits_size = width * height * 4;
        if cbits_size < 16 {
            bail!("РќРµРґРѕРїСѓСЃС‚РёРјР°СЏ РёРєРѕРЅРєР°: СЃР»РёС€РєРѕРј РјР°Р»Р°"); // СЂРµС€РёС‚СЊ РЅРµРєРѕС‚РѕСЂС‹Рµ СЃР±РѕРё
        }
        let mut cbits: Vec<u8> = Vec::new();
        cbits.resize(cbits_size as _, 0);
        let mut mbits: Vec<u8> = Vec::new();
        mbits.resize((bm_mask.bmWidthBytes * bm_mask.bmHeight) as _, 0);
        let r = GetBitmapBits(ii.0.hbmMask, mbits.len() as _, mbits.as_mut_ptr() as _);
        if r == 0 {
            bail!("РќРµ СѓРґР°Р»РѕСЃСЊ СЃРєРѕРїРёСЂРѕРІР°С‚СЊ РґР°РЅРЅС‹Рµ Р±РёС‚РјР°РїР°");
        }
        if r != (mbits.len() as i32) {
            bail!(
                "РќРµРґРѕРїСѓСЃС‚РёРјС‹Р№ СЂР°Р·РјРµСЂ Р±СѓС„РµСЂР° РјР°СЃРєРё РєСѓСЂСЃРѕСЂР°, РїРѕР»СѓС‡РµРЅРѕ {} Р±Р°Р№С‚, РѕР¶РёРґР°Р»РѕСЃСЊ {}",
                r,
                mbits.len()
            );
        }
        let do_outline;
        if ii.is_color() {
            get_rich_cursor_data(ii.0.hbmColor, width, height, &mut cbits)?;
            do_outline = fix_cursor_mask(
                &mut mbits,
                &mut cbits,
                width as _,
                height as _,
                bm_mask.bmWidthBytes as _,
            );
        } else {
            do_outline = handleMask(
                cbits.as_mut_ptr(),
                mbits.as_ptr(),
                width,
                height,
                bm_mask.bmWidthBytes,
                bm_mask.bmHeight,
            ) > 0;
        }
        if do_outline {
            let mut outline = Vec::new();
            outline.resize(((width + 2) * (height + 2) * 4) as _, 0);
            drawOutline(
                outline.as_mut_ptr(),
                cbits.as_ptr(),
                width,
                height,
                outline.len() as _,
            );
            cbits = outline;
            width += 2;
            height += 2;
            ii.0.xHotspot += 1;
            ii.0.yHotspot += 1;
        }

        Ok(CursorData {
            id: hcursor,
            colors: cbits.into(),
            hotx: ii.0.xHotspot as _,
            hoty: ii.0.yHotspot as _,
            width: width as _,
            height: height as _,
            ..Default::default()
        })
    }
}

#[inline]
fn get_bitmap(handle: HBITMAP) -> ResultType<BITMAP> {
    unsafe {
        let mut bm: BITMAP = mem::zeroed();
        if GetObjectA(
            handle as _,
            std::mem::size_of::<BITMAP>() as _,
            &mut bm as *mut BITMAP as *mut _,
        ) == FALSE
        {
            return Err(io::Error::last_os_error().into());
        }
        if bm.bmPlanes != 1 {
            bail!("РќРµРїРѕРґРґРµСЂР¶РёРІР°РµРјС‹Р№ РјРЅРѕРіРѕСЃР»РѕР№РЅС‹Р№ РєСѓСЂСЃРѕСЂ");
        }
        if bm.bmBitsPixel != 1 {
            bail!("РќРµРїРѕРґРґРµСЂР¶РёРІР°РµРјС‹Р№ С„РѕСЂРјР°С‚ РјР°СЃРєРё РєСѓСЂСЃРѕСЂР°");
        }
        Ok(bm)
    }
}

struct DC(HDC);

impl DC {
    fn new() -> ResultType<Self> {
        unsafe {
            let dc = GetDC(0 as _);
            if dc.is_null() {
                bail!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РєРѕРЅС‚РµРєСЃС‚ СЂРёСЃРѕРІР°РЅРёСЏ");
            }
            Ok(Self(dc))
        }
    }
}

impl Drop for DC {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                ReleaseDC(0 as _, self.0);
            }
        }
    }
}

struct CompatibleDC(HDC);

impl CompatibleDC {
    fn new(existing: HDC) -> ResultType<Self> {
        unsafe {
            let dc = CreateCompatibleDC(existing);
            if dc.is_null() {
                bail!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ СЃРѕРІРјРµСЃС‚РёРјС‹Р№ РєРѕРЅС‚РµРєСЃС‚ СЂРёСЃРѕРІР°РЅРёСЏ");
            }
            Ok(Self(dc))
        }
    }
}

impl Drop for CompatibleDC {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                DeleteDC(self.0);
            }
        }
    }
}

struct BitmapDC(CompatibleDC, HBITMAP);

impl BitmapDC {
    fn new(hdc: HDC, hbitmap: HBITMAP) -> ResultType<Self> {
        unsafe {
            let dc = CompatibleDC::new(hdc)?;
            let oldbitmap = SelectObject(dc.0, hbitmap as _) as HBITMAP;
            if oldbitmap.is_null() {
                bail!("РќРµ СѓРґР°Р»РѕСЃСЊ РІС‹Р±СЂР°С‚СЊ CompatibleDC");
            }
            Ok(Self(dc, oldbitmap))
        }
    }

    fn dc(&self) -> HDC {
        (self.0).0
    }
}

impl Drop for BitmapDC {
    fn drop(&mut self) {
        unsafe {
            if !self.1.is_null() {
                SelectObject((self.0).0, self.1 as _);
            }
        }
    }
}

#[inline]
fn get_rich_cursor_data(
    hbm_color: HBITMAP,
    width: i32,
    height: i32,
    out: &mut Vec<u8>,
) -> ResultType<()> {
    unsafe {
        let dc = DC::new()?;
        let bitmap_dc = BitmapDC::new(dc.0, hbm_color)?;
        if get_di_bits(out.as_mut_ptr(), bitmap_dc.dc(), hbm_color, width, height) > 0 {
            bail!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ di Р±РёС‚С‹: {}", io::Error::last_os_error());
        }
    }
    Ok(())
}

fn fix_cursor_mask(
    mbits: &mut Vec<u8>,
    cbits: &mut Vec<u8>,
    width: usize,
    height: usize,
    bm_width_bytes: usize,
) -> bool {
    let mut pix_idx = 0;
    for _ in 0..height {
        for _ in 0..width {
            if cbits[pix_idx + 3] != 0 {
                return false;
            }
            pix_idx += 4;
        }
    }

    let packed_width_bytes = (width + 7) >> 3;
    let bm_size = mbits.len();
    let c_size = cbits.len();

    // РЈРїР°РєРѕРІРєР° Рё РёРЅРІРµСЂСЃРёСЏ РґР°РЅРЅС‹С… Р±РёС‚РјР°РїР° (mbits)
    // Р·Р°РёРјСЃС‚РІРѕРІР°РЅРѕ РёР· tigervnc
    for y in 0..height {
        for x in 0..packed_width_bytes {
            let a = y * packed_width_bytes + x;
            let b = y * bm_width_bytes + x;
            if a < bm_size && b < bm_size {
                mbits[a] = !mbits[b];
            }
        }
    }

    // Р—Р°РјРµРЅР° Р±РёС‚РѕРІ "РёРЅРІРµСЂС‚РёСЂРѕРІР°РЅРЅРѕРіРѕ С„РѕРЅР°" С‡РµСЂРЅС‹Рј С†РІРµС‚РѕРј РґР»СЏ РѕР±РµСЃРїРµС‡РµРЅРёСЏ
    // РєСЂРѕСЃСЃ-РїР»Р°С‚С„РѕСЂРјРµРЅРЅРѕР№ СЃРѕРІРјРµСЃС‚РёРјРѕСЃС‚Рё. РќРµ РєСЂР°СЃРёРІРѕ, РЅРѕ РЅРµРѕР±С…РѕРґРёРјС‹Р№ РєРѕРґ.
    // Р·Р°РёРјСЃС‚РІРѕРІР°РЅРѕ РёР· tigervnc
    let bytes_row = width << 2;
    for y in 0..height {
        let mut bitmask: u8 = 0x80;
        for x in 0..width {
            let mask_idx = y * packed_width_bytes + (x >> 3);
            if mask_idx < bm_size {
                let pix_idx = y * bytes_row + (x << 2);
                if (mbits[mask_idx] & bitmask) == 0 {
                    for b1 in 0..4 {
                        let a = pix_idx + b1;
                        if a < c_size {
                            if cbits[a] != 0 {
                                mbits[mask_idx] ^= bitmask;
                                for b2 in b1..4 {
                                    let b = pix_idx + b2;
                                    if b < c_size {
                                        cbits[b] = 0x00;
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
            bitmask >>= 1;
            if bitmask == 0 {
                bitmask = 0x80;
            }
        }
    }

    // Р·Р°РёРјСЃС‚РІРѕРІР°РЅРѕ РёР· noVNC
    let mut pix_idx = 0;
    for y in 0..height {
        for x in 0..width {
            let mask_idx = y * packed_width_bytes + (x >> 3);
            let mut alpha = 255;
            if mask_idx < bm_size {
                if (mbits[mask_idx] << (x & 0x7)) & 0x80 == 0 {
                    alpha = 0;
                }
            }
            let a = cbits[pix_idx + 2];
            let b = cbits[pix_idx + 1];
            let c = cbits[pix_idx];
            cbits[pix_idx] = a;
            cbits[pix_idx + 1] = b;
            cbits[pix_idx + 2] = c;
            cbits[pix_idx + 3] = alpha;
            pix_idx += 4;
        }
    }
    return true;
}

define_windows_service!(ffi_service_main, service_main);

fn service_main(arguments: Vec<OsString>) {
    if let Err(e) = run_service(arguments) {
        log::error!("run_service failed: {}", e);
    }
}

pub fn start_os_service() {
    if let Err(e) =
        windows_service::service_dispatcher::start(crate::get_app_name(), ffi_service_main)
    {
        log::error!("start_service failed: {}", e);
    }
}

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

extern "C" {
    fn get_current_session(rdp: BOOL) -> DWORD;
    fn LaunchProcessWin(
        cmd: *const u16,
        session_id: DWORD,
        as_user: BOOL,
        show: BOOL,
        token_pid: &mut DWORD,
    ) -> HANDLE;
    fn GetSessionUserTokenWin(
        lphUserToken: LPHANDLE,
        dwSessionId: DWORD,
        as_user: BOOL,
        token_pid: &mut DWORD,
    ) -> BOOL;
    fn selectInputDesktop() -> BOOL;
    fn inputDesktopSelected() -> BOOL;
    fn is_windows_server() -> BOOL;
    fn is_windows_10_or_greater() -> BOOL;
    fn handleMask(
        out: *mut u8,
        mask: *const u8,
        width: i32,
        height: i32,
        bmWidthBytes: i32,
        bmHeight: i32,
    ) -> i32;
    fn drawOutline(out: *mut u8, in_: *const u8, width: i32, height: i32, out_size: i32);
    fn get_di_bits(out: *mut u8, dc: HDC, hbmColor: HBITMAP, width: i32, height: i32) -> i32;
    fn blank_screen(v: BOOL);
    fn win32_enable_lowlevel_keyboard(hwnd: HWND) -> i32;
    fn win32_disable_lowlevel_keyboard(hwnd: HWND);
    fn win_stop_system_key_propagate(v: BOOL);
    fn is_win_down() -> BOOL;
    fn is_local_system() -> BOOL;
    fn alloc_console_and_redirect();
    fn is_service_running_w(svc_name: *const u16) -> bool;
}

pub fn get_current_session_id(share_rdp: bool) -> DWORD {
    unsafe { get_current_session(if share_rdp { TRUE } else { FALSE }) }
}

extern "system" {
    fn BlockInput(v: BOOL) -> BOOL;
}

#[tokio::main(flavor = "current_thread")]
async fn run_service(_arguments: Vec<OsString>) -> ResultType<()> {
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        log::info!("РџРѕР»СѓС‡РµРЅРѕ СЃРѕР±С‹С‚РёРµ СѓРїСЂР°РІР»РµРЅРёСЏ СЃР»СѓР¶Р±РѕР№: {:?}", control_event);
        match control_event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop | ServiceControl::Preshutdown | ServiceControl::Shutdown => {
                send_close(crate::POSTFIX_SERVICE).ok();
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    // Р РµРіРёСЃС‚СЂР°С†РёСЏ РѕР±СЂР°Р±РѕС‚С‡РёРєР° СЃРѕР±С‹С‚РёР№ СЃРёСЃС‚РµРјРЅРѕР№ СЃР»СѓР¶Р±С‹
    let status_handle = service_control_handler::register(crate::get_app_name(), event_handler)?;

    let next_status = ServiceStatus {
        // Р”РѕР»Р¶РЅРѕ СЃРѕРІРїР°РґР°С‚СЊ СЃ С‚РµРј, С‡С‚Рѕ РІ СЂРµРµСЃС‚СЂРµ СЃРёСЃС‚РµРјРЅРѕР№ СЃР»СѓР¶Р±С‹
        service_type: SERVICE_TYPE,
        // РќРѕРІРѕРµ СЃРѕСЃС‚РѕСЏРЅРёРµ
        current_state: ServiceState::Running,
        // РџСЂРёРЅРёРјР°С‚СЊ СЃРѕР±С‹С‚РёСЏ РѕСЃС‚Р°РЅРѕРІРєРё РІРѕ РІСЂРµРјСЏ СЂР°Р±РѕС‚С‹
        controls_accepted: ServiceControlAccept::STOP,
        // РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РґР»СЏ СЃРѕРѕР±С‰РµРЅРёСЏ РѕР± РѕС€РёР±РєРµ С‚РѕР»СЊРєРѕ РїСЂРё Р·Р°РїСѓСЃРєРµ РёР»Рё РѕСЃС‚Р°РЅРѕРІРєРµ, РёРЅР°С‡Рµ РґРѕР»Р¶РЅРѕ Р±С‹С‚СЊ РЅСѓР»РµРІС‹Рј
        exit_code: ServiceExitCode::Win32(0),
        // РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ С‚РѕР»СЊРєРѕ РґР»СЏ СЃРѕСЃС‚РѕСЏРЅРёР№ РѕР¶РёРґР°РЅРёСЏ, РёРЅР°С‡Рµ РґРѕР»Р¶РЅРѕ Р±С‹С‚СЊ РЅСѓР»РµРІС‹Рј
        checkpoint: 0,
        // РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ С‚РѕР»СЊРєРѕ РґР»СЏ СЃРѕСЃС‚РѕСЏРЅРёР№ РѕР¶РёРґР°РЅРёСЏ, РёРЅР°С‡Рµ РґРѕР»Р¶РЅРѕ Р±С‹С‚СЊ РЅСѓР»РµРІС‹Рј
        wait_hint: Duration::default(),
        process_id: None,
    };

    // РЎРѕРѕР±С‰РёС‚СЊ СЃРёСЃС‚РµРјРµ, С‡С‚Рѕ СЃР»СѓР¶Р±Р° С‚РµРїРµСЂСЊ Р·Р°РїСѓС‰РµРЅР°
    status_handle.set_service_status(next_status)?;

    let mut session_id = unsafe { get_current_session(share_rdp()) };
    log::info!("РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ СЃРµСЃСЃРёРё {}", session_id);
    let mut h_process = launch_server(session_id, true).await.unwrap_or(NULL);
    let mut incoming = ipc::new_listener(crate::POSTFIX_SERVICE).await?;
    let mut stored_usid = None;
    loop {
        let sids: Vec<_> = get_available_sessions(false)
            .iter()
            .map(|e| e.sid)
            .collect();
        if !sids.contains(&session_id) || !is_share_rdp() {
            let current_active_session = unsafe { get_current_session(share_rdp()) };
            if session_id != current_active_session {
                session_id = current_active_session;
                // https://github.com/rustdesk/rustdesk/discussions/10039
                let count = ipc::get_port_forward_session_count(1000).await.unwrap_or(0);
                if count == 0 {
                    h_process = launch_server(session_id, true).await.unwrap_or(NULL);
                }
            }
        }
        let res = timeout(super::SERVICE_INTERVAL, incoming.next()).await;
        match res {
            Ok(res) => match res {
                Some(Ok(stream)) => {
                    let mut stream = ipc::Connection::new(stream);
                    if let Ok(Some(data)) = stream.next_timeout(1000).await {
                        match data {
                            ipc::Data::Close => {
                                log::info!("РїРѕР»СѓС‡РµРЅРѕ Р·Р°РєСЂС‹С‚РёРµ");
                                break;
                            }
                            ipc::Data::SAS => {
                                send_sas();
                            }
                            ipc::Data::UserSid(usid) => {
                                if let Some(usid) = usid {
                                    if session_id != usid {
                                        log::info!(
                                            "СЃРµСЃСЃРёСЏ РёР·РјРµРЅРµРЅР° СЃ {} РЅР° {}",
                                            session_id,
                                            usid
                                        );
                                        session_id = usid;
                                        stored_usid = Some(session_id);
                                        h_process =
                                            launch_server(session_id, true).await.unwrap_or(NULL);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Err(_) => {
                // С‚Р°Р№Рј-Р°СѓС‚
                unsafe {
                    let tmp = get_current_session(share_rdp());
                    if tmp == 0xFFFFFFFF {
                        continue;
                    }
                    let mut close_sent = false;
                    if tmp != session_id && stored_usid != Some(session_id) {
                        log::info!("СЃРµСЃСЃРёСЏ РёР·РјРµРЅРµРЅР° СЃ {} РЅР° {}", session_id, tmp);
                        session_id = tmp;
                        let count = ipc::get_port_forward_session_count(1000).await.unwrap_or(0);
                        if count == 0 {
                            send_close_async("").await.ok();
                            close_sent = true;
                        }
                    }
                    let mut exit_code: DWORD = 0;
                    if h_process.is_null()
                        || (GetExitCodeProcess(h_process, &mut exit_code) == TRUE
                            && exit_code != STILL_ACTIVE
                            && CloseHandle(h_process) == TRUE)
                    {
                        match launch_server(session_id, !close_sent).await {
                            Ok(ptr) => {
                                h_process = ptr;
                            }
                            Err(err) => {
                                log::error!("РќРµ СѓРґР°Р»РѕСЃСЊ Р·Р°РїСѓСЃС‚РёС‚СЊ СЃРµСЂРІРµСЂ: {}", err);
                            }
                        }
                    }
                }
            }
        }
    }

    if !h_process.is_null() {
        send_close_async("").await.ok();
        unsafe { CloseHandle(h_process) };
    }

    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

async fn launch_server(session_id: DWORD, close_first: bool) -> ResultType<HANDLE> {
    if close_first {
        // РІ СЃР»СѓС‡Р°Рµ Р·Р°РїСѓСЃРєР° РіРґРµ-С‚Рѕ РµС‰Рµ
        send_close_async("").await.ok();
    }
    let cmd = format!(
        "\"{}\" --server",
        std::env::current_exe()?.to_str().unwrap_or("")
    );
    launch_privileged_process(session_id, &cmd)
}

pub fn launch_privileged_process(session_id: DWORD, cmd: &str) -> ResultType<HANDLE> {
    use std::os::windows::ffi::OsStrExt;
    let wstr: Vec<u16> = std::ffi::OsStr::new(&cmd)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect();
    let wstr = wstr.as_ptr();
    let mut token_pid = 0;
    let h = unsafe { LaunchProcessWin(wstr, session_id, FALSE, FALSE, &mut token_pid) };
    if h.is_null() {
        log::error!(
            "РќРµ СѓРґР°Р»РѕСЃСЊ Р·Р°РїСѓСЃС‚РёС‚СЊ РїСЂРёРІРёР»РµРіРёСЂРѕРІР°РЅРЅС‹Р№ РїСЂРѕС†РµСЃСЃ: {}",
            io::Error::last_os_error()
        );
        if token_pid == 0 {
            log::error!("РќРµС‚ РїСЂРѕС†РµСЃСЃР° winlogon.exe");
        }
    }
    Ok(h)
}

pub fn run_as_user(arg: Vec<&str>) -> ResultType<Option<std::process::Child>> {
    run_exe_in_cur_session(std::env::current_exe()?.to_str().unwrap_or(""), arg, false)
}

pub fn run_exe_in_cur_session(
    exe: &str,
    arg: Vec<&str>,
    show: bool,
) -> ResultType<Option<std::process::Child>> {
    let Some(session_id) = get_current_process_session_id() else {
        bail!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ СЃРµСЃСЃРёРё С‚РµРєСѓС‰РµРіРѕ РїСЂРѕС†РµСЃСЃР°");
    };
    run_exe_in_session(exe, arg, session_id, show)
}

pub fn run_exe_in_session(
    exe: &str,
    arg: Vec<&str>,
    session_id: DWORD,
    show: bool,
) -> ResultType<Option<std::process::Child>> {
    use std::os::windows::ffi::OsStrExt;
    let cmd = format!("\"{}\" {}", exe, arg.join(" "),);
    let wstr: Vec<u16> = std::ffi::OsStr::new(&cmd)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect();
    let wstr = wstr.as_ptr();
    let mut token_pid = 0;
    let h = unsafe {
        LaunchProcessWin(
            wstr,
            session_id,
            TRUE,
            if show { TRUE } else { FALSE },
            &mut token_pid,
        )
    };
    if h.is_null() {
        if token_pid == 0 {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ Р·Р°РїСѓСЃС‚РёС‚СЊ {:?} СЃ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂРѕРј СЃРµСЃСЃРёРё {}: РЅРµС‚ РїСЂРѕС†РµСЃСЃР° {}",
                arg,
                session_id,
                EXPLORER_EXE
            );
        }
        bail!(
            "РќРµ СѓРґР°Р»РѕСЃСЊ Р·Р°РїСѓСЃС‚РёС‚СЊ {:?} СЃ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂРѕРј СЃРµСЃСЃРёРё {}: {}",
            arg,
            session_id,
            io::Error::last_os_error()
        );
    }
    Ok(None)
}

#[tokio::main(flavor = "current_thread")]
async fn send_close(postfix: &str) -> ResultType<()> {
    send_close_async(postfix).await
}

async fn send_close_async(postfix: &str) -> ResultType<()> {
    ipc::connect(1000, postfix)
        .await?
        .send(&ipc::Data::Close)
        .await?;
    // РїРѕРґРѕР¶РґР°С‚СЊ РЅРµРјРЅРѕРіРѕ РґР»СЏ Р·Р°РєСЂС‹С‚РёСЏ Рё РІС‹С…РѕРґР°
    sleep(0.1).await;
    Ok(())
}

// https://docs.microsoft.com/en-us/windows/win32/api/sas/nf-sas-sendsas
// https://www.cnblogs.com/doutu/p/4892726.html
pub fn send_sas() {
    #[link(name = "sas")]
    extern "system" {
        pub fn SendSAS(AsUser: BOOL);
    }
    unsafe {
        log::info!("РџРѕР»СѓС‡РµРЅ SAS");

        // РџСЂРѕРІРµСЂРёС‚СЊ Рё РІСЂРµРјРµРЅРЅРѕ СѓСЃС‚Р°РЅРѕРІРёС‚СЊ SoftwareSASGeneration, РµСЃР»Рё РЅСѓР¶РЅРѕ
        let mut original_value: Option<u32> = None;
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE as isize);

        if let Ok(policy_key) = hklm.open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
            KEY_READ | KEY_WRITE,
        ) {
            // РџСЂРѕС‡РёС‚Р°С‚СЊ С‚РµРєСѓС‰РµРµ Р·РЅР°С‡РµРЅРёРµ
            match policy_key.get_value::<u32, _>("SoftwareSASGeneration") {
                Ok(value) => {
                    /*
                    - 0 = None (РѕС‚РєР»СЋС‡РµРЅРѕ)
                    - 1 = Services
                    - 2 = Ease of Access applications
                    - 3 = Services and Ease of Access applications (Both)
                                      */
                    if value != 1 && value != 3 {
                        original_value = Some(value);
                        log::info!("SoftwareSASGeneration СЂР°РІРЅРѕ {}, СѓСЃС‚Р°РЅР°РІР»РёРІР°РµРј 1", value);
                        // РЈСЃС‚Р°РЅРѕРІРёС‚СЊ 1, С‡С‚РѕР±С‹ SendSAS СЂР°Р±РѕС‚Р°Р»
                        if let Err(e) = policy_key.set_value("SoftwareSASGeneration", &1u32) {
                            log::error!("РќРµ СѓРґР°Р»РѕСЃСЊ СѓСЃС‚Р°РЅРѕРІРёС‚СЊ SoftwareSASGeneration: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::info!(
                        "SoftwareSASGeneration РЅРµ РЅР°Р№РґРµРЅ РёР»Рё РѕС€РёР±РєР° С‡С‚РµРЅРёСЏ: {}, СѓСЃС‚Р°РЅР°РІР»РёРІР°РµРј 1",
                        e
                    );
                    original_value = Some(0); // РћС‚РјРµС‚РёС‚СЊ, С‡С‚Рѕ РЅСѓР¶РЅРѕ РІРѕСЃСЃС‚Р°РЅРѕРІРёС‚СЊ (СѓРґР°Р»РёС‚СЊ)
                                              // РЎРѕР·РґР°С‚СЊ Рё СѓСЃС‚Р°РЅРѕРІРёС‚СЊ 1
                    if let Err(e) = policy_key.set_value("SoftwareSASGeneration", &1u32) {
                        log::error!("РќРµ СѓРґР°Р»РѕСЃСЊ СѓСЃС‚Р°РЅРѕРІРёС‚СЊ SoftwareSASGeneration: {}", e);
                    }
                }
            }
        } else {
            log::error!("РќРµ СѓРґР°Р»РѕСЃСЊ РѕС‚РєСЂС‹С‚СЊ РєР»СЋС‡ СЂРµРµСЃС‚СЂР° РґР»СЏ SoftwareSASGeneration");
        }

        // РћС‚РїСЂР°РІРёС‚СЊ SAS
        SendSAS(FALSE);

        // Р’РѕСЃСЃС‚Р°РЅРѕРІРёС‚СЊ РѕСЂРёРіРёРЅР°Р»СЊРЅРѕРµ Р·РЅР°С‡РµРЅРёРµ, РµСЃР»Рё РёР·РјРµРЅРёР»Рё
        if let Some(original) = original_value {
            if let Ok(policy_key) = hklm.open_subkey_with_flags(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
                KEY_WRITE,
            ) {
                if original == 0 {
                    // Р Р°РЅРµРµ РЅРµ СЃСѓС‰РµСЃС‚РІРѕРІР°Р»Рѕ, СѓРґР°Р»РёС‚СЊ
                    if let Err(e) = policy_key.delete_value("SoftwareSASGeneration") {
                        log::error!("РќРµ СѓРґР°Р»РѕСЃСЊ СѓРґР°Р»РёС‚СЊ SoftwareSASGeneration: {}", e);
                    } else {
                        log::info!("РЈРґР°Р»РµРЅ SoftwareSASGeneration (РІРѕСЃСЃС‚Р°РЅРѕРІР»РµРЅРѕ РѕСЂРёРіРёРЅР°Р»СЊРЅРѕРµ СЃРѕСЃС‚РѕСЏРЅРёРµ)");
                    }
                } else {
                    // Р’РѕСЃСЃС‚Р°РЅРѕРІРёС‚СЊ РѕСЂРёРіРёРЅР°Р»СЊРЅРѕРµ Р·РЅР°С‡РµРЅРёРµ
                    if let Err(e) = policy_key.set_value("SoftwareSASGeneration", &original) {
                        log::error!(
                            "РќРµ СѓРґР°Р»РѕСЃСЊ РІРѕСЃСЃС‚Р°РЅРѕРІРёС‚СЊ SoftwareSASGeneration РІ {}: {}",
                            original,
                            e
                        );
                    } else {
                        log::info!("Р’РѕСЃСЃС‚Р°РЅРѕРІР»РµРЅРѕ SoftwareSASGeneration РІ {}", original);
                    }
                }
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref SUPPRESS: Arc<Mutex<Instant>> = Arc::new(Mutex::new(Instant::now()));
}

pub fn desktop_changed() -> bool {
    unsafe { inputDesktopSelected() == FALSE }
}

pub fn try_change_desktop() -> bool {
    unsafe {
        if inputDesktopSelected() == FALSE {
            let res = selectInputDesktop() == TRUE;
            if !res {
                let mut s = SUPPRESS.lock().unwrap();
                if s.elapsed() > std::time::Duration::from_secs(3) {
                    log::error!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРµСЂРµРєР»СЋС‡РёС‚СЊ СЂР°Р±РѕС‡РёР№ СЃС‚РѕР»: {}", io::Error::last_os_error());
                    *s = Instant::now();
                }
            } else {
                log::info!("Р Р°Р±РѕС‡РёР№ СЃС‚РѕР» РїРµСЂРµРєР»СЋС‡РµРЅ");
            }
            return res;
        }
    }
    return false;
}

fn share_rdp() -> BOOL {
    if get_reg("share_rdp") != "false" {
        TRUE
    } else {
        FALSE
    }
}

pub fn is_share_rdp() -> bool {
    share_rdp() == TRUE
}

pub fn set_share_rdp(enable: bool) {
    let (subkey, _, _, _) = get_install_info();
    let cmd = format!(
        "reg add {} /f /v share_rdp /t REG_SZ /d \"{}\"",
        subkey,
        if enable { "true" } else { "false" }
    );
    run_cmds(cmd, false, "share_rdp").ok();
}

pub fn get_current_process_session_id() -> Option<u32> {
    get_session_id_of_process(unsafe { GetCurrentProcessId() })
}

pub fn get_session_id_of_process(pid: DWORD) -> Option<u32> {
    let mut sid = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut sid) == TRUE } {
        Some(sid)
    } else {
        None
    }
}

pub fn is_physical_console_session() -> Option<bool> {
    if let Some(sid) = get_current_process_session_id() {
        let physical_console_session_id = unsafe { get_current_session(FALSE) };
        if physical_console_session_id == u32::MAX {
            return None;
        }
        return Some(physical_console_session_id == sid);
    }
    None
}

pub fn get_active_username() -> String {
    // get_active_user Р±СѓРґРµС‚ РѕС‚РґР°РІР°С‚СЊ РїСЂРёРѕСЂРёС‚РµС‚ РёРјРµРЅРё РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ РєРѕРЅСЃРѕР»Рё
    if let Some(name) = get_current_session_username() {
        return name;
    }
    if !is_root() {
        return crate::username();
    }

    extern "C" {
        fn get_active_user(path: *mut u16, n: u32, rdp: BOOL) -> u32;
    }
    let buff_size = 256;
    let mut buff: Vec<u16> = Vec::with_capacity(buff_size);
    buff.resize(buff_size, 0);
    let n = unsafe { get_active_user(buff.as_mut_ptr(), buff_size as _, share_rdp()) };
    if n == 0 {
        return "".to_owned();
    }
    let sl = unsafe { std::slice::from_raw_parts(buff.as_ptr(), n as _) };
    String::from_utf16(sl)
        .unwrap_or("??".to_owned())
        .trim_end_matches('\0')
        .to_owned()
}

fn get_current_session_username() -> Option<String> {
    let Some(sid) = get_current_process_session_id() else {
        log::error!("get_current_process_session_id failed");
        return None;
    };
    Some(get_session_username(sid))
}

fn get_session_username(session_id: u32) -> String {
    extern "C" {
        fn get_session_user_info(path: *mut u16, n: u32, session_id: u32) -> u32;
    }
    let buff_size = 256;
    let mut buff: Vec<u16> = Vec::with_capacity(buff_size);
    buff.resize(buff_size, 0);
    let n = unsafe { get_session_user_info(buff.as_mut_ptr(), buff_size as _, session_id) };
    if n == 0 {
        return "".to_owned();
    }
    let sl = unsafe { std::slice::from_raw_parts(buff.as_ptr(), n as _) };
    String::from_utf16(sl)
        .unwrap_or("".to_owned())
        .trim_end_matches('\0')
        .to_owned()
}

pub fn get_available_sessions(name: bool) -> Vec<WindowsSession> {
    extern "C" {
        fn get_available_session_ids(buf: *mut wchar_t, buf_size: c_int, include_rdp: bool);
    }
    const BUF_SIZE: c_int = 1024;
    let mut buf: Vec<wchar_t> = vec![0; BUF_SIZE as usize];

    let station_session_id_array = unsafe {
        get_available_session_ids(buf.as_mut_ptr(), BUF_SIZE, true);
        let session_ids = String::from_utf16_lossy(&buf);
        session_ids.trim_matches(char::from(0)).trim().to_string()
    };
    let mut v: Vec<WindowsSession> = vec![];
    // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-wtsgetactiveconsolesessionid
    let physical_console_sid = unsafe { get_current_session(FALSE) };
    if physical_console_sid != u32::MAX {
        let physical_console_name = if name {
            let physical_console_username = get_session_username(physical_console_sid);
            if physical_console_username.is_empty() {
                "Console".to_owned()
            } else {
                format!("Console: {physical_console_username}")
            }
        } else {
            "".to_owned()
        };
        v.push(WindowsSession {
            sid: physical_console_sid,
            name: physical_console_name,
            ..Default::default()
        });
    }
    // https://learn.microsoft.com/en-us/previous-versions//cc722458(v=technet.10)?redirectedfrom=MSDN
    for type_session_id in station_session_id_array.split(",") {
        let split: Vec<_> = type_session_id.split(":").collect();
        if split.len() == 2 {
            if let Ok(sid) = split[1].parse::<u32>() {
                if !v.iter().any(|e| (*e).sid == sid) {
                    let name = if name {
                        let name = get_session_username(sid);
                        if name.is_empty() {
                            split[0].to_string()
                        } else {
                            format!("{}: {}", split[0], name)
                        }
                    } else {
                        "".to_owned()
                    };
                    v.push(WindowsSession {
                        sid,
                        name,
                        ..Default::default()
                    });
                }
            }
        }
    }
    if name {
        let mut name_count: HashMap<String, usize> = HashMap::new();
        for session in &v {
            *name_count.entry(session.name.clone()).or_insert(0) += 1;
        }
        let current_sid = get_current_process_session_id().unwrap_or_default();
        for e in v.iter_mut() {
            let running = e.sid == current_sid && current_sid != 0;
            if name_count.get(&e.name).map(|v| *v).unwrap_or_default() > 1 {
                e.name = format!("{} (sid = {})", e.name, e.sid);
            }
            if running {
                e.name = format!("{} (running)", e.name);
            }
        }
    }
    v
}

pub fn get_active_user_home() -> Option<PathBuf> {
    let username = get_active_username();
    if !username.is_empty() {
        let drive = std::env::var("SystemDrive").unwrap_or("C:".to_owned());
        let home = PathBuf::from(format!("{}\\Users\\{}", drive, username));
        if home.exists() {
            return Some(home);
        }
    }
    None
}

pub fn is_prelogin() -> bool {
    let Some(username) = get_current_session_username() else {
        return false;
    };
    username.is_empty() || username == "SYSTEM"
}

// `is_logon_ui()` РёРіРЅРѕСЂРёСЂСѓРµС‚ РЅРµСЃРєРѕР»СЊРєРѕ СЃРµСЃСЃРёР№ СЃРµР№С‡Р°СЃ.
// РћРЅ С‚РѕР»СЊРєРѕ РїСЂРѕРІРµСЂСЏРµС‚, СЃСѓС‰РµСЃС‚РІСѓРµС‚ Р»Рё "LogonUI.exe".
//
// Р•СЃР»Рё РµСЃС‚СЊ РЅРµСЃРєРѕР»СЊРєРѕ СЃРµСЃСЃРёР№ (Р·Р°СЂРµРіРёСЃС‚СЂРёСЂРѕРІР°РЅРЅС‹Рµ РїРѕР»СЊР·РѕРІР°С‚РµР»Рё),
// РЅРµРєРѕС‚РѕСЂС‹Рµ РЅР° СЌРєСЂР°РЅРµ РІС…РѕРґР°, Р° РґСЂСѓРіРёРµ РЅРµС‚.
// РўРѕРіРґР° СЌС‚Р° С„СѓРЅРєС†РёСЏ РјРѕР¶РµС‚ РЅРµ СЂР°Р±РѕС‚Р°С‚СЊ РїСЂР°РІРёР»СЊРЅРѕ, РµСЃР»Рё СЃРµСЃСЃРёСЏ, РєРѕС‚РѕСЂСѓСЋ РјС‹ С…РѕС‚РёРј РѕР±СЂР°Р±РѕС‚Р°С‚СЊ (РїРѕРґРєР»СЋС‡РёС‚СЊ), РЅРµ РЅР° СЌРєСЂР°РЅРµ РІС…РѕРґР°.
// РќРѕ СЌС‚Рѕ СЂРµРґРєРёР№ СЃР»СѓС‡Р°Р№ Рё РЅРµ РјРѕР¶РµС‚ Р±С‹С‚СЊ РїСЂРѕСЃС‚Рѕ РѕР±СЂР°Р±РѕС‚Р°РЅ, РїРѕСЌС‚РѕРјСѓ РїРѕРєР° РЅРµ Р±СѓРґРµС‚ РѕР±СЂР°Р±Р°С‚С‹РІР°С‚СЊСЃСЏ.
#[inline]
pub fn is_logon_ui() -> ResultType<bool> {
    let pids = get_pids("LogonUI.exe")?;
    Ok(!pids.is_empty())
}

pub fn is_root() -> bool {
    // https://stackoverflow.com/questions/4023586/correct-way-to-find-out-if-a-service-is-running-as-the-system-user
    unsafe { is_local_system() == TRUE }
}

pub fn lock_screen() {
    extern "system" {
        pub fn LockWorkStation() -> BOOL;
    }
    unsafe {
        LockWorkStation();
    }
}

const IS1: &str = "{54E86BC2-6C85-41F3-A9EB-1A94AC9B1F93}_is1";

fn get_subkey(name: &str, wow: bool) -> String {
    let tmp = format!(
        "HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}",
        name
    );
    if wow {
        tmp.replace("Microsoft", "Wow6432Node\\Microsoft")
    } else {
        tmp
    }
}

fn get_valid_subkey() -> String {
    let subkey = get_subkey(IS1, false);
    if !get_reg_of(&subkey, "InstallLocation").is_empty() {
        return subkey;
    }
    let subkey = get_subkey(IS1, true);
    if !get_reg_of(&subkey, "InstallLocation").is_empty() {
        return subkey;
    }
    let app_name = crate::get_app_name();
    let subkey = get_subkey(&app_name, true);
    if !get_reg_of(&subkey, "InstallLocation").is_empty() {
        return subkey;
    }
    return get_subkey(&app_name, false);
}

// Р’РѕР·РІСЂР°С‰Р°РµС‚ РїР°СЂР°РјРµС‚СЂС‹ СѓСЃС‚Р°РЅРѕРІРєРё, РєСЂРѕРјРµ InstallLocation.
pub fn get_install_options() -> String {
    let app_name = crate::get_app_name();
    let subkey = format!(".{}", app_name.to_lowercase());
    let mut opts = HashMap::new();

    let desktop_shortcuts = get_reg_of_hkcr(&subkey, REG_NAME_INSTALL_DESKTOPSHORTCUTS);
    if let Some(desktop_shortcuts) = desktop_shortcuts {
        opts.insert(REG_NAME_INSTALL_DESKTOPSHORTCUTS, desktop_shortcuts);
    }
    let start_menu_shortcuts = get_reg_of_hkcr(&subkey, REG_NAME_INSTALL_STARTMENUSHORTCUTS);
    if let Some(start_menu_shortcuts) = start_menu_shortcuts {
        opts.insert(REG_NAME_INSTALL_STARTMENUSHORTCUTS, start_menu_shortcuts);
    }
    let printer = get_reg_of_hkcr(&subkey, REG_NAME_INSTALL_PRINTER);
    if let Some(printer) = printer {
        opts.insert(REG_NAME_INSTALL_PRINTER, printer);
    }
    serde_json::to_string(&opts).unwrap_or("{}".to_owned())
}

// Р­С‚Р° С„СѓРЅРєС†РёСЏ РІРѕР·РІСЂР°С‰Р°РµС‚ Option<String>, РїРѕС‚РѕРјСѓ С‡С‚Рѕ Р·РЅР°С‡РµРЅРёРµ СЂРµРµСЃС‚СЂР° РјРѕР¶РµС‚ Р±С‹С‚СЊ РїСѓСЃС‚С‹Рј.
fn get_reg_of_hkcr(subkey: &str, name: &str) -> Option<String> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT as isize);
    if let Ok(tmp) = hkcr.open_subkey(subkey.replace("HKEY_CLASSES_ROOT\\", "")) {
        return tmp.get_value(name).ok();
    }
    None
}

pub fn get_install_info() -> (String, String, String, String) {
    get_install_info_with_subkey(get_valid_subkey())
}

fn get_default_install_info() -> (String, String, String, String) {
    get_install_info_with_subkey(get_subkey(&crate::get_app_name(), false))
}

fn get_default_install_path() -> String {
    let mut pf = "C:\\Program Files".to_owned();
    if let Ok(x) = std::env::var("ProgramFiles") {
        if std::path::Path::new(&x).exists() {
            pf = x;
        }
    }
    #[cfg(target_pointer_width = "32")]
    {
        let tmp = pf.replace("Program Files", "Program Files (x86)");
        if std::path::Path::new(&tmp).exists() {
            pf = tmp;
        }
    }
    format!("{}\\{}", pf, crate::get_app_name())
}

pub fn check_update_broker_process() -> ResultType<()> {
    let process_exe = win_topmost_window::INJECTED_PROCESS_EXE;
    let origin_process_exe = win_topmost_window::ORIGIN_PROCESS_EXE;

    let exe_file = std::env::current_exe()?;
    let Some(cur_dir) = exe_file.parent() else {
        bail!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ СЂРѕРґРёС‚РµР»СЊСЃРєРёР№ РєР°С‚Р°Р»РѕРі С‚РµРєСѓС‰РµРіРѕ exe-С„Р°Р№Р»Р°");
    };
    let cur_exe = cur_dir.join(process_exe);

    // РџСЂРёРЅСѓРґРёС‚РµР»СЊРЅРѕРµ РѕР±РЅРѕРІР»РµРЅРёРµ exe Р±СЂРѕРєРµСЂР°, РµСЃР»Рё РЅРµ СѓРґР°Р»РѕСЃСЊ РїСЂРѕРІРµСЂРёС‚СЊ РІСЂРµРјСЏ РёР·РјРµРЅРµРЅРёСЏ.
    let cmds = format!(
        "
        chcp 65001
        taskkill /F /IM {process_exe}
        copy /Y \"{origin_process_exe}\" \"{cur_exe}\"
    ",
        cur_exe = cur_exe.to_string_lossy(),
    );

    if !std::path::Path::new(&cur_exe).exists() {
        run_cmds(cmds, false, "update_broker")?;
        return Ok(());
    }

    let ori_modified = fs::metadata(origin_process_exe)?.modified()?;
    if let Ok(metadata) = fs::metadata(&cur_exe) {
        if let Ok(cur_modified) = metadata.modified() {
            if cur_modified == ori_modified {
                return Ok(());
            } else {
                log::info!(
                    "РїСЂРѕС†РµСЃСЃ Р±СЂРѕРєРµСЂР° РѕР±РЅРѕРІР»РµРЅ, РІСЂРµРјСЏ РёР·РјРµРЅРµРЅРёСЏ СЃ {:?} РЅР° {:?}",
                    cur_modified,
                    ori_modified
                );
            }
        }
    }

    run_cmds(cmds, false, "update_broker")?;

    Ok(())
}

fn get_install_info_with_subkey(subkey: String) -> (String, String, String, String) {
    let mut path = get_reg_of(&subkey, "InstallLocation");
    if path.is_empty() {
        path = get_default_install_path();
    }
    path = path.trim_end_matches('\\').to_owned();
    let start_menu = format!(
        "%ProgramData%\\Microsoft\\Windows\\Start Menu\\Programs\\{}",
        crate::get_app_name()
    );
    let exe = format!("{}\\{}.exe", path, crate::get_app_name());
    (subkey, path, start_menu, exe)
}

pub fn copy_raw_cmd(src_raw: &str, _raw: &str, _path: &str) -> ResultType<String> {
    let main_raw = format!(
        "XCOPY \"{}\" \"{}\" /Y /E /H /C /I /K /R /Z",
        PathBuf::from(src_raw)
            .parent()
            .ok_or(anyhow!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ СЂРѕРґРёС‚РµР»СЊСЃРєРёР№ РєР°С‚Р°Р»РѕРі {src_raw}"))?
            .to_string_lossy()
            .to_string(),
        _path
    );
    return Ok(main_raw);
}

pub fn copy_exe_cmd(src_exe: &str, exe: &str, path: &str) -> ResultType<String> {
    let main_exe = copy_raw_cmd(src_exe, exe, path)?;
    Ok(format!(
        "
        {main_exe}
        copy /Y \"{ORIGIN_PROCESS_EXE}\" \"{path}\\{broker_exe}\"
        ",
        ORIGIN_PROCESS_EXE = win_topmost_window::ORIGIN_PROCESS_EXE,
        broker_exe = win_topmost_window::INJECTED_PROCESS_EXE,
    ))
}

fn get_after_install(
    exe: &str,
    reg_value_start_menu_shortcuts: Option<String>,
    reg_value_desktop_shortcuts: Option<String>,
    reg_value_printer: Option<String>,
) -> String {
    let app_name = crate::get_app_name();
    let ext = app_name.to_lowercase();

    // reg delete HKEY_CURRENT_USER\Software\Classes РґР»СЏ
    // https://github.com/rustdesk/rustdesk/commit/f4bdfb6936ae4804fc8ab1cf560db192622ad01a
    // Рё https://github.com/leanflutter/uni_links_desktop/blob/1b72b0226cec9943ca8a84e244c149773f384e46/lib/src/protocol_registrar_impl_windows.dart#L30
    let hcu = RegKey::predef(HKEY_CURRENT_USER as isize);
    hcu.delete_subkey_all(format!("Software\\Classes\\{}", exe))
        .ok();

    let desktop_shortcuts = reg_value_desktop_shortcuts
        .map(|v| {
            format!("reg add HKEY_CLASSES_ROOT\\.{ext} /f /v {REG_NAME_INSTALL_DESKTOPSHORTCUTS} /t REG_SZ /d \"{v}\"")
        })
        .unwrap_or_default();
    let start_menu_shortcuts = reg_value_start_menu_shortcuts
        .map(|v| {
            format!(
                "reg add HKEY_CLASSES_ROOT\\.{ext} /f /v {REG_NAME_INSTALL_STARTMENUSHORTCUTS} /t REG_SZ /d \"{v}\""
            )
        })
        .unwrap_or_default();
    let reg_printer = reg_value_printer
        .map(|v| {
            format!(
                "reg add HKEY_CLASSES_ROOT\\.{ext} /f /v {REG_NAME_INSTALL_PRINTER} /t REG_SZ /d \"{v}\""
            )
        })
        .unwrap_or_default();

    format!("
    chcp 65001
    reg add HKEY_CLASSES_ROOT\\.{ext} /f
    {desktop_shortcuts}
    {start_menu_shortcuts}
    {reg_printer}
    reg add HKEY_CLASSES_ROOT\\.{ext}\\DefaultIcon /f
    reg add HKEY_CLASSES_ROOT\\.{ext}\\DefaultIcon /f /ve /t REG_SZ  /d \"\\\"{exe}\\\",0\"
    reg add HKEY_CLASSES_ROOT\\.{ext}\\shell /f
    reg add HKEY_CLASSES_ROOT\\.{ext}\\shell\\open /f
    reg add HKEY_CLASSES_ROOT\\.{ext}\\shell\\open\\command /f
    reg add HKEY_CLASSES_ROOT\\.{ext}\\shell\\open\\command /f /ve /t REG_SZ /d \"\\\"{exe}\\\" --play \\\"%%1\\\"\"
    reg add HKEY_CLASSES_ROOT\\{ext} /f
    reg add HKEY_CLASSES_ROOT\\{ext} /f /v \"URL Protocol\" /t REG_SZ /d \"\"
    reg add HKEY_CLASSES_ROOT\\{ext}\\shell /f
    reg add HKEY_CLASSES_ROOT\\{ext}\\shell\\open /f
    reg add HKEY_CLASSES_ROOT\\{ext}\\shell\\open\\command /f
    reg add HKEY_CLASSES_ROOT\\{ext}\\shell\\open\\command /f /ve /t REG_SZ /d \"\\\"{exe}\\\" \\\"%%1\\\"\"
    netsh advfirewall firewall add rule name=\"{app_name} Service\" dir=out action=allow program=\"{exe}\" enable=yes
    netsh advfirewall firewall add rule name=\"{app_name} Service\" dir=in action=allow program=\"{exe}\" enable=yes
    {create_service}
    reg add HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System /f /v SoftwareSASGeneration /t REG_DWORD /d 1
    ", create_service=get_create_service(&exe))
}

pub fn install_me(options: &str, path: String, silent: bool, debug: bool) -> ResultType<()> {
    let uninstall_str = get_uninstall(false, false);
    let mut path = path.trim_end_matches('\\').to_owned();
    let (subkey, _path, start_menu, exe) = get_default_install_info();
    let mut exe = exe;
    if path.is_empty() {
        path = _path;
    } else {
        exe = exe.replace(&_path, &path);
    }
    let mut version_major = "0";
    let mut version_minor = "0";
    let mut version_build = "0";
    let versions: Vec<&str> = crate::VERSION.split(".").collect();
    if versions.len() > 0 {
        version_major = versions[0];
    }
    if versions.len() > 1 {
        version_minor = versions[1];
    }
    if versions.len() > 2 {
        version_build = versions[2];
    }
    let app_name = crate::get_app_name();

    let tmp_path = std::env::temp_dir().to_string_lossy().to_string();
    let mk_shortcut = write_cmds(
        format!(
            "
Set oWS = WScript.CreateObject(\"WScript.Shell\")
sLinkFile = \"{tmp_path}\\{app_name}.lnk\"

Set oLink = oWS.CreateShortcut(sLinkFile)
    oLink.TargetPath = \"{exe}\"
oLink.Save
        "
        ),
        "vbs",
        "mk_shortcut",
    )?
    .to_str()
    .unwrap_or("")
    .to_owned();
    // https://superuser.com/questions/392061/how-to-make-a-shortcut-from-cmd
    let uninstall_shortcut = write_cmds(
        format!(
            "
Set oWS = WScript.CreateObject(\"WScript.Shell\")
sLinkFile = \"{tmp_path}\\Uninstall {app_name}.lnk\"
Set oLink = oWS.CreateShortcut(sLinkFile)
    oLink.TargetPath = \"{exe}\"
    oLink.Arguments = \"--uninstall\"
    oLink.IconLocation = \"msiexec.exe\"
oLink.Save
        "
        ),
        "vbs",
        "uninstall_shortcut",
    )?
    .to_str()
    .unwrap_or("")
    .to_owned();
    let tray_shortcut = get_tray_shortcut(&exe, &tmp_path)?;
    let mut reg_value_desktop_shortcuts = "0".to_owned();
    let mut reg_value_start_menu_shortcuts = "0".to_owned();
    let mut reg_value_printer = "0".to_owned();
    let mut shortcuts = Default::default();
    if options.contains("desktopicon") {
        shortcuts = format!(
            "copy /Y \"{}\\{}.lnk\" \"%PUBLIC%\\Desktop\\\"",
            tmp_path,
            crate::get_app_name()
        );
        reg_value_desktop_shortcuts = "1".to_owned();
    }
    if options.contains("startmenu") {
        shortcuts = format!(
            "{shortcuts}
md \"{start_menu}\"
copy /Y \"{tmp_path}\\{app_name}.lnk\" \"{start_menu}\\\"
copy /Y \"{tmp_path}\\Uninstall {app_name}.lnk\" \"{start_menu}\\\"
     "
        );
        reg_value_start_menu_shortcuts = "1".to_owned();
    }
    let install_printer = options.contains("printer") && is_win_10_or_greater();
    if install_printer {
        reg_value_printer = "1".to_owned();
    }

    let meta = std::fs::symlink_metadata(std::env::current_exe()?)?;
    let size = meta.len() / 1024;
    // https://docs.microsoft.com/zh-cn/windows/win32/msi/uninstall-registry-key?redirectedfrom=MSDNa
    // https://www.windowscentral.com/how-edit-registry-using-command-prompt-windows-10
    // https://www.tenforums.com/tutorials/70903-add-remove-allowed-apps-through-windows-firewall-windows-10-a.html
    // РџСЂРёРјРµС‡Р°РЅРёРµ: Р±РµР· if exist, bat РјРѕР¶РµС‚ РІС‹Р№С‚Рё Р·Р°СЂР°РЅРµРµ РЅР° РЅРµРєРѕС‚РѕСЂС‹С… Windows7 https://github.com/rustdesk/rustdesk/issues/895
    let dels = format!(
        "
if exist \"{mk_shortcut}\" del /f /q \"{mk_shortcut}\"
if exist \"{uninstall_shortcut}\" del /f /q \"{uninstall_shortcut}\"
if exist \"{tray_shortcut}\" del /f /q \"{tray_shortcut}\"
if exist \"{tmp_path}\\{app_name}.lnk\" del /f /q \"{tmp_path}\\{app_name}.lnk\"
if exist \"{tmp_path}\\Uninstall {app_name}.lnk\" del /f /q \"{tmp_path}\\Uninstall {app_name}.lnk\"
if exist \"{tmp_path}\\{app_name} Tray.lnk\" del /f /q \"{tmp_path}\\{app_name} Tray.lnk\"
        "
    );
    let src_exe = std::env::current_exe()?.to_str().unwrap_or("").to_string();

    // РїРѕС‚РµРЅС†РёР°Р»СЊРЅР°СЏ РѕС€РёР±РєР° Р·РґРµСЃСЊ: РµСЃР»Рё run_cmd РѕС‚РјРµРЅРµРЅ, РЅРѕ С„Р°Р№Р» РєРѕРЅС„РёРіСѓСЂР°С†РёРё РёР·РјРµРЅРµРЅ.
    if let Some(lic) = get_license() {
        Config::set_option("key".into(), lic.key);
        Config::set_option("custom-rendezvous-server".into(), lic.host);
        Config::set_option("api-server".into(), lic.api);
    }

    let tray_shortcuts = if config::is_outgoing_only() {
        "".to_owned()
    } else {
        format!("
cscript \"{tray_shortcut}\"
copy /Y \"{tmp_path}\\{app_name} Tray.lnk\" \"%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\\"
")
    };

    let install_remote_printer = if install_printer {
        // РќРµС‚ РЅРµРѕР±С…РѕРґРёРјРѕСЃС‚Рё РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ `|| true` Р·РґРµСЃСЊ.
        // РЎРєСЂРёРїС‚ РЅРµ РІС‹Р№РґРµС‚, РґР°Р¶Рµ РµСЃР»Рё `--install-remote-printer` РїР°РЅРёРєСѓРµС‚.
        format!("\"{}\" --install-remote-printer", &src_exe)
    } else if is_win_10_or_greater() {
        format!("\"{}\" --uninstall-remote-printer", &src_exe)
    } else {
        "".to_owned()
    };

    // РџРѕРјРЅРёС‚Рµ РїСЂРѕРІРµСЂРёС‚СЊ, РЅСѓР¶РЅРѕ Р»Рё РёР·РјРµРЅРёС‚СЊ `update_me`, РµСЃР»Рё РёР·РјРµРЅСЏРµС‚Рµ `cmds`.
    // РќРµС‚ РЅРµРѕР±С…РѕРґРёРјРѕСЃС‚Рё СЃР»РёРІР°С‚СЊ СЃСѓС‰РµСЃС‚РІСѓСЋС‰РёР№ РґСѓР±Р»РёСЂСѓСЋС‰РёР№СЃСЏ РєРѕРґ, РїРѕС‚РѕРјСѓ С‡С‚Рѕ РєРѕРґ РІ СЌС‚РёС… РґРІСѓС… С„СѓРЅРєС†РёСЏС… СЃР»РёС€РєРѕРј РєСЂРёС‚РёС‡РµРЅ.
    // РќРѕРІС‹Р№ РєРѕРґ РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ РЅР°РїРёСЃР°РЅ РІ РѕР±С‰РµР№ С„СѓРЅРєС†РёРё.
    let cmds = format!(
        "
{uninstall_str}
chcp 65001
md \"{path}\"
{copy_exe}
reg add {subkey} /f
reg add {subkey} /f /v DisplayIcon /t REG_SZ /d \"{exe}\"
reg add {subkey} /f /v DisplayName /t REG_SZ /d \"{app_name}\"
reg add {subkey} /f /v DisplayVersion /t REG_SZ /d \"{version}\"
reg add {subkey} /f /v Version /t REG_SZ /d \"{version}\"
reg add {subkey} /f /v BuildDate /t REG_SZ /d \"{build_date}\"
reg add {subkey} /f /v InstallLocation /t REG_SZ /d \"{path}\"
reg add {subkey} /f /v Publisher /t REG_SZ /d \"{app_name}\"
reg add {subkey} /f /v VersionMajor /t REG_DWORD /d {version_major}
reg add {subkey} /f /v VersionMinor /t REG_DWORD /d {version_minor}
reg add {subkey} /f /v VersionBuild /t REG_DWORD /d {version_build}
reg add {subkey} /f /v UninstallString /t REG_SZ /d \"\\\"{exe}\\\" --uninstall\"
reg add {subkey} /f /v EstimatedSize /t REG_DWORD /d {size}
reg add {subkey} /f /v WindowsInstaller /t REG_DWORD /d 0
cscript \"{mk_shortcut}\"
cscript \"{uninstall_shortcut}\"
{tray_shortcuts}
{shortcuts}
copy /Y \"{tmp_path}\\Uninstall {app_name}.lnk\" \"{path}\\\"
{dels}
{import_config}
{after_install}
{install_remote_printer}
{sleep}
    ",
        version = crate::VERSION.replace("-", "."),
        build_date = crate::BUILD_DATE,
        after_install = get_after_install(
            &exe,
            Some(reg_value_start_menu_shortcuts),
            Some(reg_value_desktop_shortcuts),
            Some(reg_value_printer)
        ),
        sleep = if debug { "timeout 300" } else { "" },
        dels = if debug { "" } else { &dels },
        copy_exe = copy_exe_cmd(&src_exe, &exe, &path)?,
        import_config = get_import_config(&exe),
    );
    run_cmds(cmds, debug, "install")?;
    run_after_run_cmds(silent);
    Ok(())
}

pub fn run_after_install() -> ResultType<()> {
    let (_, _, _, exe) = get_install_info();
    run_cmds(
        get_after_install(&exe, None, None, None),
        true,
        "after_install",
    )
}

pub fn run_before_uninstall() -> ResultType<()> {
    run_cmds(get_before_uninstall(true), true, "before_install")
}

fn get_before_uninstall(kill_self: bool) -> String {
    let app_name = crate::get_app_name();
    let ext = app_name.to_lowercase();
    let filter = if kill_self {
        "".to_string()
    } else {
        format!(" /FI \"PID ne {}\"", get_current_pid())
    };
    format!(
        "
    chcp 65001
    sc stop {app_name}
    sc delete {app_name}
    taskkill /F /IM {broker_exe}
    taskkill /F /IM {app_name}.exe{filter}
    reg delete HKEY_CLASSES_ROOT\\.{ext} /f
    reg delete HKEY_CLASSES_ROOT\\{ext} /f
    netsh advfirewall firewall delete rule name=\"{app_name} Service\"
    ",
        broker_exe = WIN_TOPMOST_INJECTED_PROCESS_EXE,
    )
}

/// РџРѕСЃС‚СЂРѕРµРЅРёРµ СЃС‚СЂРѕРєРё РєРѕРјР°РЅРґС‹ РґРµРёРЅСЃС‚Р°Р»Р»СЏС†РёРё РґР»СЏ РїСЂРёР»РѕР¶РµРЅРёСЏ.
///
/// # РџР°СЂР°РјРµС‚СЂС‹
/// - `kill_self`: РљРѕРјР°РЅРґР° Р±СѓРґРµС‚ СѓР±РёРІР°С‚СЊ РїСЂРѕС†РµСЃСЃ С‚РµРєСѓС‰РµРіРѕ РёРјРµРЅРё РїСЂРёР»РѕР¶РµРЅРёСЏ. Р•СЃР»Рё `true`, РѕРЅР° СѓР±СЊРµС‚
///   С‚РµРєСѓС‰РёР№ РїСЂРѕС†РµСЃСЃ С‚Р°РєР¶Рµ. Р•СЃР»Рё `false`, РѕРЅР° РёСЃРєР»СЋС‡РёС‚ С‚РµРєСѓС‰РёР№ РїСЂРѕС†РµСЃСЃ РёР· РєРѕРјР°РЅРґС‹ СѓР±РёР№СЃС‚РІР°.
/// - `uninstall_printer`: Р•СЃР»Рё `true`, РІРєР»СЋС‡Р°РµС‚ РєРѕРјР°РЅРґС‹ РґР»СЏ РґРµРёРЅСЃС‚Р°Р»Р»СЏС†РёРё СѓРґР°Р»РµРЅРЅРѕРіРѕ РїСЂРёРЅС‚РµСЂР°.
///
/// # Р”РµС‚Р°Р»Рё
/// РџР°СЂР°РјРµС‚СЂ `uninstall_printer` РѕРїСЂРµРґРµР»СЏРµС‚, РІРєР»СЋС‡РµРЅР° Р»Рё РєРѕРјР°РЅРґР° РґРµРёРЅСЃС‚Р°Р»Р»СЏС†РёРё СѓРґР°Р»РµРЅРЅРѕРіРѕ РїСЂРёРЅС‚РµСЂР°
/// РІ СЃРіРµРЅРµСЂРёСЂРѕРІР°РЅРЅС‹Р№ СЃРєСЂРёРїС‚ РґРµРёРЅСЃС‚Р°Р»Р»СЏС†РёРё. Р•СЃР»Рё `uninstall_printer` - `false`, РєРѕРјР°РЅРґР°, СЃРІСЏР·Р°РЅРЅР°СЏ СЃ РїСЂРёРЅС‚РµСЂРѕРј,
/// РѕРїСѓСЃРєР°РµС‚СЃСЏ РёР· СЃРєСЂРёРїС‚Р°.
fn get_uninstall(kill_self: bool, uninstall_printer: bool) -> String {
    let reg_uninstall_string = get_reg("UninstallString");
    if reg_uninstall_string.to_lowercase().contains("msiexec.exe") {
        return reg_uninstall_string;
    }

    let mut uninstall_cert_cmd = "".to_string();
    let mut uninstall_printer_cmd = "".to_string();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_path) = exe.to_str() {
            uninstall_cert_cmd = format!("\"{}\" --uninstall-cert", exe_path);
            if uninstall_printer {
                uninstall_printer_cmd = format!("\"{}\" --uninstall-remote-printer", &exe_path);
            }
        }
    }
    let (subkey, path, start_menu, _) = get_install_info();
    format!(
        "
    {before_uninstall}
    {uninstall_printer_cmd}
    {uninstall_cert_cmd}
    reg delete {subkey} /f
    {uninstall_amyuni_idd}
    if exist \"{path}\" rd /s /q \"{path}\"
    if exist \"{start_menu}\" rd /s /q \"{start_menu}\"
    if exist \"%PUBLIC%\\Desktop\\{app_name}.lnk\" del /f /q \"%PUBLIC%\\Desktop\\{app_name}.lnk\"
    if exist \"%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\{app_name} Tray.lnk\" del /f /q \"%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\{app_name} Tray.lnk\"
    ",
        before_uninstall=get_before_uninstall(kill_self),
        uninstall_amyuni_idd=get_uninstall_amyuni_idd(),
        app_name = crate::get_app_name(),
    )
}

pub fn uninstall_me(kill_self: bool) -> ResultType<()> {
    run_cmds(get_uninstall(kill_self, true), true, "uninstall")
}

fn write_cmds(cmds: String, ext: &str, tip: &str) -> ResultType<std::path::PathBuf> {
    let mut cmds = cmds;
    let mut tmp = std::env::temp_dir();
    // РљРѕРіРґР° РєР°С‚Р°Р»РѕРі СЃРѕРґРµСЂР¶РёС‚ СЌС‚Рё СЃРёРјРІРѕР»С‹, bat-С„Р°Р№Р» РЅРµ РІС‹РїРѕР»РЅРёС‚СЃСЏ РІ РїРѕРІС‹С€РµРЅРЅРѕРј СЂРµР¶РёРјРµ.
    if vec!["&", "@", "^"]
        .drain(..)
        .any(|s| tmp.to_string_lossy().to_string().contains(s))
    {
        if let Ok(dir) = user_accessible_folder() {
            tmp = dir;
        }
    }
    tmp.push(format!("{}_{}.{}", crate::get_app_name(), tip, ext));
    let mut file = std::fs::File::create(&tmp)?;
    if ext == "bat" {
        let tmp2 = get_undone_file(&tmp)?;
        std::fs::File::create(&tmp2).ok();
        cmds = format!(
            "
{cmds}
if exist \"{path}\" del /f /q \"{path}\"
",
            path = tmp2.to_string_lossy()
        );
    }
    // РІ СЃР»СѓС‡Р°Рµ, РµСЃР»Рё cmds СЃРјРµС€Р°РЅС‹ СЃ \r\n Рё \n, СѓР±РµРґРёС‚РµСЃСЊ, С‡С‚Рѕ РІСЃРµ Р·Р°РєР°РЅС‡РёРІР°СЋС‚СЃСЏ \r\n
    // РЅР° РЅРµРєРѕС‚РѕСЂС‹С… windows \r\n С‚СЂРµР±СѓРµС‚СЃСЏ РґР»СЏ РІС‹РїРѕР»РЅРµРЅРёСЏ cmd-С„Р°Р№Р»Р°
    cmds = cmds.replace("\r\n", "\n").replace("\n", "\r\n");
    if ext == "vbs" {
        let mut v: Vec<u16> = cmds.encode_utf16().collect();
        // utf8 -> utf16le, РєРѕС‚РѕСЂС‹Р№ РїРѕРґРґРµСЂР¶РёРІР°РµС‚ С‚РѕР»СЊРєРѕ vbs
        file.write_all(to_le(&mut v))?;
    } else {
        file.write_all(cmds.as_bytes())?;
    }
    file.sync_all()?;
    return Ok(tmp);
}

fn to_le(v: &mut [u16]) -> &[u8] {
    for b in v.iter_mut() {
        *b = b.to_le()
    }
    unsafe { v.align_to().1 }
}

fn get_undone_file(tmp: &Path) -> ResultType<PathBuf> {
    Ok(tmp.with_file_name(format!(
        "{}.undone",
        tmp.file_name()
            .ok_or(anyhow!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РёРјСЏ С„Р°Р№Р»Р° {:?}", tmp))?
            .to_string_lossy()
    )))
}

fn run_cmds(cmds: String, show: bool, tip: &str) -> ResultType<()> {
    let tmp = write_cmds(cmds, "bat", tip)?;
    let tmp2 = get_undone_file(&tmp)?;
    let tmp_fn = tmp.to_str().unwrap_or("");
    // https://github.com/rustdesk/rustdesk/issues/6786#issuecomment-1879655410
    // РЈРєР°Р¶РёС‚Рµ cmd.exe СЏРІРЅРѕ, С‡С‚РѕР±С‹ РёР·Р±РµР¶Р°С‚СЊ Р·Р°РјРµРЅС‹ cmd-РєРѕРјР°РЅРґ.
    let res = runas::Command::new("cmd.exe")
        .args(&["/C", &tmp_fn])
        .show(show)
        .force_prompt(true)
        .status();
    if !show {
        allow_err!(std::fs::remove_file(tmp));
    }
    let _ = res?;
    if tmp2.exists() {
        allow_err!(std::fs::remove_file(tmp2));
        bail!("{} РЅРµ СѓРґР°Р»РѕСЃСЊ", tip);
    }
    Ok(())
}

pub fn toggle_blank_screen(v: bool) {
    let v = if v { TRUE } else { FALSE };
    unsafe {
        blank_screen(v);
    }
}

pub fn block_input(v: bool) -> (bool, String) {
    let v = if v { TRUE } else { FALSE };
    unsafe {
        if BlockInput(v) == TRUE {
            (true, "".to_owned())
        } else {
            (false, format!("РћС€РёР±РєР°: {}", io::Error::last_os_error()))
        }
    }
}

pub fn add_recent_document(path: &str) {
    extern "C" {
        fn AddRecentDocument(path: *const u16);
    }
    use std::os::windows::ffi::OsStrExt;
    let wstr: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect();
    let wstr = wstr.as_ptr();
    unsafe {
        AddRecentDocument(wstr);
    }
}

pub fn is_installed() -> bool {
    let (_, _, _, exe) = get_install_info();
    std::fs::metadata(exe).is_ok()
}

pub fn get_reg(name: &str) -> String {
    let (subkey, _, _, _) = get_install_info();
    get_reg_of(&subkey, name)
}

fn get_reg_of(subkey: &str, name: &str) -> String {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE as isize);
    if let Ok(tmp) = hklm.open_subkey(subkey.replace("HKEY_LOCAL_MACHINE\\", "")) {
        if let Ok(v) = tmp.get_value(name) {
            return v;
        }
    }
    "".to_owned()
}

pub fn get_license_from_exe_name() -> ResultType<CustomServer> {
    let mut exe = std::env::current_exe()?.to_str().unwrap_or("").to_owned();
    // РµСЃР»Рё РѕРїСЂРµРґРµР»РµРЅР° Р·Р°РїРёСЃСЊ portable appname, Р·Р°РјРµРЅРёС‚СЊ РѕСЂРёРіРёРЅР°Р»СЊРЅРѕРµ РёРјСЏ РёСЃРїРѕР»РЅСЏРµРјРѕРіРѕ С„Р°Р№Р»Р° РЅР° РЅРµРµ.
    if let Ok(portable_exe) = std::env::var(PORTABLE_APPNAME_RUNTIME_ENV_KEY) {
        exe = portable_exe;
    }
    get_custom_server_from_string(&exe)
}

// РњС‹ РЅРµ РјРѕР¶РµРј РЅР°РїСЂСЏРјСѓСЋ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ `RegKey::set_value` РґР»СЏ РѕР±РЅРѕРІР»РµРЅРёСЏ Р·РЅР°С‡РµРЅРёСЏ СЂРµРµСЃС‚СЂР°, РїРѕС‚РѕРјСѓ С‡С‚Рѕ РѕРЅРѕ РїСЂРѕРІР°Р»РёС‚СЃСЏ СЃ `ERROR_ACCESS_DENIED`
// РўР°Рє С‡С‚Рѕ РјС‹ РґРѕР»Р¶РЅС‹ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ `run_cmds` РґР»СЏ РѕР±РЅРѕРІР»РµРЅРёСЏ Р·РЅР°С‡РµРЅРёСЏ СЂРµРµСЃС‚СЂР°.
pub fn update_install_option(k: &str, v: &str) -> ResultType<()> {
    // РќРµ РѕР±РЅРѕРІР»СЏС‚СЊ СЂРµРµСЃС‚СЂ, РµСЃР»Рё РЅРµ СѓСЃС‚Р°РЅРѕРІР»РµРЅРѕ РёР»Рё РЅРµ СЃРµСЂРІРµСЂРЅС‹Р№ РїСЂРѕС†РµСЃСЃ.
    if !is_installed() || !crate::is_server() {
        return Ok(());
    }
    let app_name = crate::get_app_name();
    let ext = app_name.to_lowercase();
    let cmds =
        format!("chcp 65001 && reg add HKEY_CLASSES_ROOT\\.{ext} /f /v {k} /t REG_SZ /d \"{v}\"");
    run_cmds(cmds, false, "update_install_option")?;
    Ok(())
}

#[inline]
pub fn is_win_server() -> bool {
    unsafe { is_windows_server() > 0 }
}

#[inline]
pub fn is_win_10_or_greater() -> bool {
    unsafe { is_windows_10_or_greater() > 0 }
}

pub fn bootstrap() -> bool {
    if let Ok(lic) = get_license_from_exe_name() {
        *config::EXE_RENDEZVOUS_SERVER.write().unwrap() = lic.host.clone();
    }

    #[cfg(debug_assertions)]
    {
        true
    }
    #[cfg(not(debug_assertions))]
    {
        // Р­С‚Р° С„СѓРЅРєС†РёСЏ РІС‹Р·РѕРІРµС‚ `'sciter.dll' РЅРµ РЅР°Р№РґРµРЅ РЅРё РІ PATH, РЅРё СЂСЏРґРѕРј СЃ С‚РµРєСѓС‰РёРј РёСЃРїРѕР»РЅСЏРµРјС‹Рј С„Р°Р№Р»РѕРј.` РїСЂРё РѕС‚Р»Р°РґРєРµ RustDesk.
        // Р’С‹Р·С‹РІР°С‚СЊ set_safe_load_dll() С‚РѕР»СЊРєРѕ РЅР° Windows 10 РёР»Рё РІС‹С€Рµ
        if is_win_10_or_greater() {
            set_safe_load_dll()
        } else {
            true
        }
    }
}

#[cfg(not(debug_assertions))]
fn set_safe_load_dll() -> bool {
    if !unsafe { set_default_dll_directories() } {
        return false;
    }

    // `SetDllDirectoryW` РЅРёРєРѕРіРґР° РЅРµ РґРѕР»Р¶РЅР° РїСЂРѕРІР°Р»РёС‚СЊСЃСЏ.
    // https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setdlldirectoryw
    if unsafe { SetDllDirectoryW(wide_string("").as_ptr()) == FALSE } {
        eprintln!("SetDllDirectoryW РЅРµ СѓРґР°Р»Р°СЃСЊ: {}", io::Error::last_os_error());
        return false;
    }

    true
}

// https://docs.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-setdefaultdlldirectories
#[cfg(not(debug_assertions))]
unsafe fn set_default_dll_directories() -> bool {
    let module = LoadLibraryExW(
        wide_string("Kernel32.dll").as_ptr(),
        0 as _,
        LOAD_LIBRARY_SEARCH_SYSTEM32,
    );
    if module.is_null() {
        return false;
    }

    match CString::new("SetDefaultDllDirectories") {
        Err(e) => {
            eprintln!("CString::new РЅРµ СѓРґР°Р»Р°СЃСЊ: {}", e);
            return false;
        }
        Ok(func_name) => {
            let func = GetProcAddress(module, func_name.as_ptr());
            if func.is_null() {
                eprintln!("GetProcAddress РЅРµ СѓРґР°Р»Р°СЃСЊ: {}", io::Error::last_os_error());
                return false;
            }
            type SetDefaultDllDirectories = unsafe extern "system" fn(DWORD) -> BOOL;
            let func: SetDefaultDllDirectories = std::mem::transmute(func);
            if func(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_USER_DIRS) == FALSE {
                eprintln!(
                    "SetDefaultDllDirectories РЅРµ СѓРґР°Р»Р°СЃСЊ: {}",
                    io::Error::last_os_error()
                );
                return false;
            }
        }
    }
    true
}

pub fn create_shortcut(id: &str) -> ResultType<()> {
    let exe = std::env::current_exe()?.to_str().unwrap_or("").to_owned();
    let shortcut = write_cmds(
        format!(
            "
Set oWS = WScript.CreateObject(\"WScript.Shell\")
strDesktop = oWS.SpecialFolders(\"Desktop\")
Set objFSO = CreateObject(\"Scripting.FileSystemObject\")
sLinkFile = objFSO.BuildPath(strDesktop, \"{id}.lnk\")
Set oLink = oWS.CreateShortcut(sLinkFile)
    oLink.TargetPath = \"{exe}\"
    oLink.Arguments = \"--connect {id}\"
oLink.Save
        "
        ),
        "vbs",
        "connect_shortcut",
    )?
    .to_str()
    .unwrap_or("")
    .to_owned();
    std::process::Command::new("cscript")
        .arg(&shortcut)
        .output()?;
    allow_err!(std::fs::remove_file(shortcut));
    Ok(())
}

pub fn enable_lowlevel_keyboard(hwnd: HWND) {
    let ret = unsafe { win32_enable_lowlevel_keyboard(hwnd) };
    if ret != 0 {
        log::error!("РЎР±РѕР№ Р·Р°С…РІР°С‚Р° РєР»Р°РІРёР°С‚СѓСЂС‹");
        return;
    }
}

pub fn disable_lowlevel_keyboard(hwnd: HWND) {
    unsafe { win32_disable_lowlevel_keyboard(hwnd) };
}

pub fn stop_system_key_propagate(v: bool) {
    unsafe { win_stop_system_key_propagate(if v { TRUE } else { FALSE }) };
}

pub fn get_win_key_state() -> bool {
    unsafe { is_win_down() == TRUE }
}

pub fn quit_gui() {
    std::process::exit(0);
    // unsafe { PostQuitMessage(0) }; // РєР°РєРёРј-С‚Рѕ РѕР±СЂР°Р·РѕРј РЅРµ СЂР°Р±РѕС‚Р°РµС‚
}

pub fn get_user_token(session_id: u32, as_user: bool) -> HANDLE {
    let mut token = NULL as HANDLE;
    unsafe {
        let mut _token_pid = 0;
        if FALSE
            == GetSessionUserTokenWin(
                &mut token as _,
                session_id,
                if as_user { TRUE } else { FALSE },
                &mut _token_pid,
            )
        {
            NULL as _
        } else {
            token
        }
    }
}

pub fn run_background(exe: &str, arg: &str) -> ResultType<bool> {
    let wexe = wide_string(exe);
    let warg;
    unsafe {
        let ret = ShellExecuteW(
            NULL as _,
            NULL as _,
            wexe.as_ptr() as _,
            if arg.is_empty() {
                NULL as _
            } else {
                warg = wide_string(arg);
                warg.as_ptr() as _
            },
            NULL as _,
            SW_HIDE,
        );
        return Ok(ret as i32 > 32);
    }
}

pub fn run_uac(exe: &str, arg: &str) -> ResultType<bool> {
    let wop = wide_string("runas");
    let wexe = wide_string(exe);
    let warg;
    unsafe {
        let ret = ShellExecuteW(
            NULL as _,
            wop.as_ptr() as _,
            wexe.as_ptr() as _,
            if arg.is_empty() {
                NULL as _
            } else {
                warg = wide_string(arg);
                warg.as_ptr() as _
            },
            NULL as _,
            SW_SHOWNORMAL,
        );
        return Ok(ret as i32 > 32);
    }
}

pub fn check_super_user_permission() -> ResultType<bool> {
    run_uac(
        std::env::current_exe()?
            .to_string_lossy()
            .to_string()
            .as_str(),
        "--version",
    )
}

pub fn elevate(arg: &str) -> ResultType<bool> {
    run_uac(
        std::env::current_exe()?
            .to_string_lossy()
            .to_string()
            .as_str(),
        arg,
    )
}

pub fn run_as_system(arg: &str) -> ResultType<()> {
    let exe = std::env::current_exe()?.to_string_lossy().to_string();
    if impersonate_system::run_as_system(&exe, arg).is_err() {
        bail!(format!("РќРµ СѓРґР°Р»РѕСЃСЊ Р·Р°РїСѓСЃС‚РёС‚СЊ {} РєР°Рє СЃРёСЃС‚РµРјСѓ", exe));
    }
    Ok(())
}

pub fn elevate_or_run_as_system(is_setup: bool, is_elevate: bool, is_run_as_system: bool) {
    // РёР·Р±РµР¶Р°С‚СЊ РІРѕР·РјРѕР¶РЅРѕРіРѕ СЂРµРєСѓСЂСЃРёРІРЅРѕРіРѕ Р·Р°РїСѓСЃРєР° РёР·-Р·Р° РЅРµСѓРґР°С‡РЅРѕРіРѕ Р·Р°РїСѓСЃРєР°.
    log::info!(
        "elevate: {} -> {:?}, run_as_system: {} -> {}",
        is_elevate,
        is_elevated(None),
        is_run_as_system,
        crate::username(),
    );
    let arg_elevate = if is_setup {
        "--noinstall --elevate"
    } else {
        "--elevate"
    };
    let arg_run_as_system = if is_setup {
        "--noinstall --run-as-system"
    } else {
        "--run-as-system"
    };
    if is_root() {
        if is_run_as_system {
            log::info!("Р·Р°РїСѓСЃС‚РёС‚СЊ РїРѕСЂС‚Р°С‚РёРІРЅСѓСЋ СЃР»СѓР¶Р±Сѓ");
            crate::portable_service::server::run_portable_service();
        }
    } else {
        match is_elevated(None) {
            Ok(elevated) => {
                if elevated {
                    if !is_run_as_system {
                        if run_as_system(arg_run_as_system).is_ok() {
                            std::process::exit(0);
                        } else {
                            log::error!(
                                "РќРµ СѓРґР°Р»РѕСЃСЊ Р·Р°РїСѓСЃС‚РёС‚СЊ РєР°Рє СЃРёСЃС‚РµРјСѓ, РѕС€РёР±РєР° {}",
                                io::Error::last_os_error()
                            );
                        }
                    }
                } else {
                    if !is_elevate {
                        if let Ok(true) = elevate(arg_elevate) {
                            std::process::exit(0);
                        } else {
                            log::error!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕРІС‹СЃРёС‚СЊ РїСЂР°РІР°, РѕС€РёР±РєР° {}", io::Error::last_os_error());
                        }
                    }
                }
            }
            Err(_) => log::error!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ СЃС‚Р°С‚СѓСЃ РїРѕРІС‹С€РµРЅРёСЏ, РѕС€РёР±РєР° {}",
                io::Error::last_os_error()
            ),
        }
    }
}

pub fn is_elevated(process_id: Option<DWORD>) -> ResultType<bool> {
    use hbb_common::platform::windows::RAIIHandle;
    unsafe {
        let handle: HANDLE = match process_id {
            Some(process_id) => OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, process_id),
            None => GetCurrentProcess(),
        };
        if handle == NULL {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РѕС‚РєСЂС‹С‚СЊ РїСЂРѕС†РµСЃСЃ, РѕС€РёР±РєР° {}",
                io::Error::last_os_error()
            )
        }
        let _handle = RAIIHandle(handle);
        let mut token: HANDLE = mem::zeroed();
        if OpenProcessToken(handle, TOKEN_QUERY, &mut token) == FALSE {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РѕС‚РєСЂС‹С‚СЊ С‚РѕРєРµРЅ РїСЂРѕС†РµСЃСЃР°, РѕС€РёР±РєР° {}",
                io::Error::last_os_error()
            )
        }
        let _token = RAIIHandle(token);
        let mut token_elevation: TOKEN_ELEVATION = mem::zeroed();
        let mut size: DWORD = 0;
        if GetTokenInformation(
            token,
            TokenElevation,
            (&mut token_elevation) as *mut _ as *mut c_void,
            mem::size_of::<TOKEN_ELEVATION>() as _,
            &mut size,
        ) == FALSE
        {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РёРЅС„РѕСЂРјР°С†РёСЋ Рѕ С‚РѕРєРµРЅРµ, РѕС€РёР±РєР° {}",
                io::Error::last_os_error()
            )
        }

        Ok(token_elevation.TokenIsElevated != 0)
    }
}

pub fn is_foreground_window_elevated() -> ResultType<bool> {
    unsafe {
        let mut process_id: DWORD = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut process_id);
        if process_id == 0 {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ processId, РѕС€РёР±РєР° {}",
                io::Error::last_os_error()
            )
        }
        is_elevated(Some(process_id))
    }
}

fn get_current_pid() -> u32 {
    unsafe { GetCurrentProcessId() }
}

pub fn get_double_click_time() -> u32 {
    unsafe { GetDoubleClickTime() }
}

pub fn wide_string(s: &str) -> Vec<u16> {
    use std::os::windows::prelude::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect()
}

/// РѕС‚РїСЂР°РІРёС‚СЊ СЃРѕРѕР±С‰РµРЅРёРµ С‚РµРєСѓС‰РµРјСѓ РїРѕРєР°Р·Р°РЅРЅРѕРјСѓ РѕРєРЅСѓ
pub fn send_message_to_hnwd(
    class_name: &str,
    window_name: &str,
    dw_data: usize,
    data: &str,
    show_window: bool,
) -> bool {
    unsafe {
        let class_name_utf16 = wide_string(class_name);
        let window_name_utf16 = wide_string(window_name);
        let window = FindWindowW(class_name_utf16.as_ptr(), window_name_utf16.as_ptr());
        if window.is_null() {
            log::warn!("РЅРµС‚ С‚Р°РєРѕРіРѕ РѕРєРЅР° {}:{}", class_name, window_name);
            return false;
        }
        let mut data_struct = COPYDATASTRUCT::default();
        data_struct.dwData = dw_data;
        let mut data_zero: String = data.chars().chain(Some('\0').into_iter()).collect();
        println!("РѕС‚РїСЂР°РІРёС‚СЊ {:?}", data_zero);
        data_struct.cbData = data_zero.len() as _;
        data_struct.lpData = data_zero.as_mut_ptr() as _;
        SendMessageW(
            window,
            WM_COPYDATA,
            0,
            &data_struct as *const COPYDATASTRUCT as _,
        );
        if show_window {
            ShowWindow(window, SW_NORMAL);
            SetForegroundWindow(window);
        }
    }
    return true;
}

pub fn get_logon_user_token(user: &str, pwd: &str) -> ResultType<HANDLE> {
    let user_split = user.split("\\").collect::<Vec<&str>>();
    let wuser = wide_string(user_split.get(1).unwrap_or(&user));
    let wpc = wide_string(user_split.get(0).unwrap_or(&""));
    let wpwd = wide_string(pwd);
    let mut ph_token: HANDLE = std::ptr::null_mut();
    let res = unsafe {
        LogonUserW(
            wuser.as_ptr(),
            wpc.as_ptr(),
            wpwd.as_ptr(),
            LOGON32_LOGON_INTERACTIVE,
            LOGON32_PROVIDER_DEFAULT,
            &mut ph_token as _,
        )
    };
    if res == FALSE {
        bail!(
            "РќРµ СѓРґР°Р»РѕСЃСЊ РІРѕР№С‚Рё РїРѕР»СЊР·РѕРІР°С‚РµР»РµРј {}: {}",
            user,
            std::io::Error::last_os_error()
        );
    } else {
        if ph_token.is_null() {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РІРѕР№С‚Рё РїРѕР»СЊР·РѕРІР°С‚РµР»РµРј {}: {}",
                user,
                std::io::Error::last_os_error()
            );
        }
        Ok(ph_token)
    }
}

// РЈР±РµРґРёС‚СЊСЃСЏ, С‡С‚Рѕ РІРѕР·РІСЂР°С‰Р°РµРјС‹Р№ С‚РѕРєРµРЅ СЏРІР»СЏРµС‚СЃСЏ РїРµСЂРІРёС‡РЅС‹Рј С‚РѕРєРµРЅРѕРј.
// Р•СЃР»Рё РїСЂРµРґРѕСЃС‚Р°РІР»РµРЅРЅС‹Р№ С‚РѕРєРµРЅ СЏРІР»СЏРµС‚СЃСЏ С‚РѕРєРµРЅРѕРј РёРјРїРµСЂСЃРѕРЅР°С†РёРё, РѕРЅ РґСѓР±Р»РёСЂСѓРµС‚ РµРіРѕ РІ РїРµСЂРІРёС‡РЅС‹Р№ С‚РѕРєРµРЅ.
// Р•СЃР»Рё РїСЂРµРґРѕСЃС‚Р°РІР»РµРЅРЅС‹Р№ С‚РѕРєРµРЅ СѓР¶Рµ СЏРІР»СЏРµС‚СЃСЏ РїРµСЂРІРёС‡РЅС‹Рј С‚РѕРєРµРЅРѕРј, РѕРЅ РІРѕР·РІСЂР°С‰Р°РµС‚ РµРіРѕ РєР°Рє РµСЃС‚СЊ.
// Р’С‹Р·С‹РІР°СЋС‰РёР№ РѕС‚РІРµС‡Р°РµС‚ Р·Р° Р·Р°РєСЂС‹С‚РёРµ РІРѕР·РІСЂР°С‰РµРЅРЅРѕРіРѕ РґРµСЃРєСЂРёРїС‚РѕСЂР° С‚РѕРєРµРЅР°.
pub fn ensure_primary_token(user_token: HANDLE) -> ResultType<HANDLE> {
    if user_token.is_null() || user_token == INVALID_HANDLE_VALUE {
        bail!("РџСЂРµРґРѕСЃС‚Р°РІР»РµРЅ РЅРµРґРѕРїСѓСЃС‚РёРјС‹Р№ С‚РѕРєРµРЅ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ");
    }

    unsafe {
        let mut token_type: TOKEN_TYPE = 0;
        let mut return_length: DWORD = 0;

        if GetTokenInformation(
            user_token,
            TokenType,
            &mut token_type as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_TYPE>() as DWORD,
            &mut return_length,
        ) == FALSE
        {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ С‚РёРї С‚РѕРєРµРЅР°, РѕС€РёР±РєР° {}",
                io::Error::last_os_error()
            );
        }

        if token_type == TokenImpersonation {
            let mut duplicate_token: HANDLE = std::ptr::null_mut();
            let dup_res = DuplicateToken(user_token, SecurityImpersonation, &mut duplicate_token);
            CloseHandle(user_token);
            if dup_res == FALSE {
                bail!(
                    "РќРµ СѓРґР°Р»РѕСЃСЊ РґСѓР±Р»РёСЂРѕРІР°С‚СЊ С‚РѕРєРµРЅ, РѕС€РёР±РєР° {}",
                    io::Error::last_os_error()
                );
            }
            Ok(duplicate_token)
        } else {
            Ok(user_token)
        }
    }
}

pub fn is_user_token_admin(user_token: HANDLE) -> ResultType<bool> {
    if user_token.is_null() || user_token == INVALID_HANDLE_VALUE {
        bail!("РџСЂРµРґРѕСЃС‚Р°РІР»РµРЅ РЅРµРґРѕРїСѓСЃС‚РёРјС‹Р№ С‚РѕРєРµРЅ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ");
    }

    unsafe {
        let mut dw_size: DWORD = 0;
        GetTokenInformation(
            user_token,
            TokenGroups,
            std::ptr::null_mut(),
            0,
            &mut dw_size,
        );

        let last_error = GetLastError();
        if last_error != ERROR_INSUFFICIENT_BUFFER {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ СЂР°Р·РјРµСЂ Р±СѓС„РµСЂР° РіСЂСѓРїРї С‚РѕРєРµРЅР°, РѕС€РёР±РєР°: {}",
                last_error
            );
        }
        if dw_size == 0 {
            bail!("Р Р°Р·РјРµСЂ Р±СѓС„РµСЂР° РіСЂСѓРїРї С‚РѕРєРµРЅР° РЅСѓР»РµРІРѕР№");
        }

        let mut buffer = vec![0u8; dw_size as usize];
        if GetTokenInformation(
            user_token,
            TokenGroups,
            buffer.as_mut_ptr() as *mut _,
            dw_size,
            &mut dw_size,
        ) == FALSE
        {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РёРЅС„РѕСЂРјР°С†РёСЋ Рѕ РіСЂСѓРїРїР°С… С‚РѕРєРµРЅР°, РѕС€РёР±РєР°: {}",
                io::Error::last_os_error()
            );
        }

        let p_token_groups = buffer.as_ptr() as *const TOKEN_GROUPS;
        let group_count = (*p_token_groups).GroupCount;

        if group_count == 0 {
            return Ok(false);
        }

        let mut nt_authority: SID_IDENTIFIER_AUTHORITY = SID_IDENTIFIER_AUTHORITY {
            Value: SECURITY_NT_AUTHORITY,
        };
        let mut administrators_group: PSID = std::ptr::null_mut();
        if AllocateAndInitializeSid(
            &mut nt_authority,
            2,
            SECURITY_BUILTIN_DOMAIN_RID,
            DOMAIN_ALIAS_RID_ADMINS,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut administrators_group,
        ) == FALSE
        {
            bail!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РІС‹РґРµР»РёС‚СЊ SID РіСЂСѓРїРїС‹ Р°РґРјРёРЅРёСЃС‚СЂР°С‚РѕСЂРѕРІ, РѕС€РёР±РєР°: {}",
                io::Error::last_os_error()
            );
        }
        if administrators_group.is_null() {
            bail!("РќРµ СѓРґР°Р»РѕСЃСЊ СЃРѕР·РґР°С‚СЊ SID РіСЂСѓРїРїС‹ Р°РґРјРёРЅРёСЃС‚СЂР°С‚РѕСЂРѕРІ");
        }

        let mut is_admin = false;
        let groups =
            std::slice::from_raw_parts((*p_token_groups).Groups.as_ptr(), group_count as usize);
        for group in groups {
            if EqualSid(administrators_group, group.Sid) == TRUE {
                is_admin = true;
                break;
            }
        }

        if !administrators_group.is_null() {
            FreeSid(administrators_group);
        }

        Ok(is_admin)
    }
}

pub fn create_process_with_logon(user: &str, pwd: &str, exe: &str, arg: &str) -> ResultType<()> {
    let last_error_table = HashMap::from([
        (
            ERROR_LOGON_FAILURE,
            "РРјСЏ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ РёР»Рё РїР°СЂРѕР»СЊ РЅРµРІРµСЂРЅС‹.",
        ),
        (ERROR_ACCESS_DENIED, "Р”РѕСЃС‚СѓРї Р·Р°РїСЂРµС‰РµРЅ."),
    ]);

    unsafe {
        let user_split = user.split("\\").collect::<Vec<&str>>();
        let wuser = wide_string(user_split.get(1).unwrap_or(&user));
        let wpc = wide_string(user_split.get(0).unwrap_or(&""));
        let wpwd = wide_string(pwd);
        let cmd = if arg.is_empty() {
            format!("\"{}\"", exe)
        } else {
            format!("\"{}\" {}", exe, arg)
        };
        let mut wcmd = wide_string(&cmd);
        let mut si: STARTUPINFOW = mem::zeroed();
        si.wShowWindow = SW_HIDE as _;
        si.lpDesktop = NULL as _;
        si.cb = std::mem::size_of::<STARTUPINFOW>() as _;
        si.dwFlags = STARTF_USESHOWWINDOW;
        let mut pi: PROCESS_INFORMATION = mem::zeroed();
        let wexe = wide_string(exe);
        if FALSE
            == CreateProcessWithLogonW(
                wuser.as_ptr(),
                wpc.as_ptr(),
                wpwd.as_ptr(),
                LOGON_WITH_PROFILE,
                wexe.as_ptr(),
                wcmd.as_mut_ptr(),
                CREATE_UNICODE_ENVIRONMENT,
                NULL,
                NULL as _,
                &mut si as *mut STARTUPINFOW,
                &mut pi as *mut PROCESS_INFORMATION,
            )
        {
            let last_error = GetLastError();
            bail!(
                "CreateProcessWithLogonW РЅРµ СѓРґР°Р»Р°СЃСЊ : \"{}\", РѕС€РёР±РєР° {}",
                last_error_table
                    .get(&last_error)
                    .unwrap_or(&"РќРµРёР·РІРµСЃС‚РЅР°СЏ РѕС€РёР±РєР°"),
                io::Error::from_raw_os_error(last_error as _)
            );
        }
    }
    return Ok(());
}

pub fn set_path_permission(dir: &Path, permission: &str) -> ResultType<()> {
    std::process::Command::new("icacls")
        .arg(dir.as_os_str())
        .arg("/grant")
        .arg(format!("*S-1-1-0:(OI)(CI){}", permission))
        .arg("/T")
        .spawn()?;
    Ok(())
}

#[inline]
fn str_to_device_name(name: &str) -> [u16; 32] {
    let mut device_name: Vec<u16> = wide_string(name);
    if device_name.len() < 32 {
        device_name.resize(32, 0);
    }
    let mut result = [0; 32];
    result.copy_from_slice(&device_name[..32]);
    result
}

pub fn resolutions(name: &str) -> Vec<Resolution> {
    unsafe {
        let mut dm: DEVMODEW = std::mem::zeroed();
        let mut v = vec![];
        let mut num = 0;
        let device_name = str_to_device_name(name);
        loop {
            if EnumDisplaySettingsW(device_name.as_ptr(), num, &mut dm) == 0 {
                break;
            }
            let r = Resolution {
                width: dm.dmPelsWidth as _,
                height: dm.dmPelsHeight as _,
                ..Default::default()
            };
            if !v.contains(&r) {
                v.push(r);
            }
            num += 1;
        }
        v
    }
}

pub fn current_resolution(name: &str) -> ResultType<Resolution> {
    let device_name = str_to_device_name(name);
    unsafe {
        let mut dm: DEVMODEW = std::mem::zeroed();
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as _;
        if EnumDisplaySettingsW(device_name.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm) == 0 {
            bail!(
                "РЅРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ С‚РµРєСѓС‰РµРµ СЂР°Р·СЂРµС€РµРЅРёРµ, РѕС€РёР±РєР° {}",
                io::Error::last_os_error()
            );
        }
        let r = Resolution {
            width: dm.dmPelsWidth as _,
            height: dm.dmPelsHeight as _,
            ..Default::default()
        };
        Ok(r)
    }
}

pub(super) fn change_resolution_directly(
    name: &str,
    width: usize,
    height: usize,
) -> ResultType<()> {
    let device_name = str_to_device_name(name);
    unsafe {
        let mut dm: DEVMODEW = std::mem::zeroed();
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as _;
        dm.dmPelsWidth = width as _;
        dm.dmPelsHeight = height as _;
        dm.dmFields = DM_PELSHEIGHT | DM_PELSWIDTH;
        let res = ChangeDisplaySettingsExW(
            device_name.as_ptr(),
            &mut dm,
            NULL as _,
            CDS_UPDATEREGISTRY | CDS_GLOBAL | CDS_RESET,
            NULL,
        );
        if res != DISP_CHANGE_SUCCESSFUL {
            bail!(
                "ChangeDisplaySettingsExW РЅРµ СѓРґР°Р»Р°СЃСЊ, res={}, РѕС€РёР±РєР° {}",
                res,
                io::Error::last_os_error()
            );
        }
        Ok(())
    }
}

pub fn user_accessible_folder() -> ResultType<PathBuf> {
    let disk = std::env::var("SystemDrive").unwrap_or("C:".to_string());
    let dir1 = PathBuf::from(format!("{}\\ProgramData", disk));
    // РџР РРњР•Р§РђРќРР•: "C:\Windows\Temp" С‚СЂРµР±СѓРµС‚ РїРѕСЃС‚РѕСЏРЅРЅРѕР№ Р°РІС‚РѕСЂРёР·Р°С†РёРё.
    let dir2 = PathBuf::from(format!("{}\\Windows\\Temp", disk));
    let dir;
    if dir1.exists() {
        dir = dir1;
    } else if dir2.exists() {
        dir = dir2;
    } else {
        bail!("РЅРµС‚ РґРѕРїСѓСЃС‚РёРјРѕР№ РїР°РїРєРё, РґРѕСЃС‚СѓРїРЅРѕР№ РїРѕР»СЊР·РѕРІР°С‚РµР»СЋ");
    }
    Ok(dir)
}

#[inline]
pub fn uninstall_cert() -> ResultType<()> {
    cert::uninstall_cert()
}

mod cert {
    use hbb_common::ResultType;

    extern "C" {
        fn DeleteRustDeskTestCertsW();
    }
    pub fn uninstall_cert() -> ResultType<()> {
        unsafe {
            DeleteRustDeskTestCertsW();
        }
        Ok(())
    }
}

#[inline]
pub fn get_char_from_vk(vk: u32) -> Option<char> {
    get_char_from_unicode(get_unicode_from_vk(vk)?)
}

pub fn get_char_from_unicode(unicode: u16) -> Option<char> {
    let buff = [unicode];
    if let Some(chr) = String::from_utf16(&buff[..1]).ok()?.chars().next() {
        if chr.is_control() {
            return None;
        } else {
            Some(chr)
        }
    } else {
        None
    }
}

pub fn get_unicode_from_vk(vk: u32) -> Option<u16> {
    const BUF_LEN: i32 = 32;
    let mut buff = [0_u16; BUF_LEN as usize];
    let buff_ptr = buff.as_mut_ptr();
    let len = unsafe {
        let current_window_thread_id = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
        let layout = GetKeyboardLayout(current_window_thread_id);

        // refs: https://github.com/rustdesk-org/rdev/blob/25a99ce71ab42843ad253dd51e6a35e83e87a8a4/src/windows/keyboard.rs#L115
        let press_state = 129;
        let mut state: [BYTE; 256] = [0; 256];
        let shift_left = rdev::get_modifier(rdev::Key::ShiftLeft);
        let shift_right = rdev::get_modifier(rdev::Key::ShiftRight);
        if shift_left {
            state[VK_LSHIFT as usize] = press_state;
        }
        if shift_right {
            state[VK_RSHIFT as usize] = press_state;
        }
        if shift_left || shift_right {
            state[VK_SHIFT as usize] = press_state;
        }
        ToUnicodeEx(vk, 0x00, &state as _, buff_ptr, BUF_LEN, 0, layout)
    };
    if len == 1 {
        Some(buff[0])
    } else {
        None
    }
}

pub fn is_process_consent_running() -> ResultType<bool> {
    let output = std::process::Command::new("cmd")
        .args(&["/C", "tasklist | findstr consent.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    Ok(output.status.success() && !output.stdout.is_empty())
}

pub struct WakeLock(u32);
// РќРµ СѓРґР°Р»РѕСЃСЊ СЃРєРѕРјРїРёР»РёСЂРѕРІР°С‚СЊ keepawake-rs РЅР° i686
impl WakeLock {
    pub fn new(display: bool, idle: bool, sleep: bool) -> Self {
        let mut flag = ES_CONTINUOUS;
        if display {
            flag |= ES_DISPLAY_REQUIRED;
        }
        if idle {
            flag |= ES_SYSTEM_REQUIRED;
        }
        if sleep {
            flag |= ES_AWAYMODE_REQUIRED;
        }
        unsafe { SetThreadExecutionState(flag) };
        WakeLock(flag)
    }

    pub fn set_display(&mut self, display: bool) -> ResultType<()> {
        let flag = if display {
            self.0 | ES_DISPLAY_REQUIRED
        } else {
            self.0 & !ES_DISPLAY_REQUIRED
        };
        if flag != self.0 {
            unsafe { SetThreadExecutionState(flag) };
            self.0 = flag;
        }
        Ok(())
    }
}

impl Drop for WakeLock {
    fn drop(&mut self) {
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }
}

pub fn uninstall_service(show_new_window: bool, _: bool) -> bool {
    log::info!("Р”РµРёРЅСЃС‚Р°Р»Р»СЏС†РёСЏ СЃР»СѓР¶Р±С‹...");
    let filter = format!(" /FI \"PID ne {}\"", get_current_pid());
    Config::set_option("stop-service".into(), "Y".into());
    let cmds = format!(
        "
    chcp 65001
    sc stop {app_name}
    sc delete {app_name}
    if exist \"%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\{app_name} Tray.lnk\" del /f /q \"%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\{app_name} Tray.lnk\"
    taskkill /F /IM {broker_exe}
    taskkill /F /IM {app_name}.exe{filter}
    ",
        app_name = crate::get_app_name(),
        broker_exe = WIN_TOPMOST_INJECTED_PROCESS_EXE,
    );
    if let Err(err) = run_cmds(cmds, false, "uninstall") {
        Config::set_option("stop-service".into(), "".into());
        log::debug!("{err}");
        return true;
    }
    run_after_run_cmds(!show_new_window);
    std::process::exit(0);
}

pub fn install_service() -> bool {
    log::info!("РЈСЃС‚Р°РЅРѕРІРєР° СЃР»СѓР¶Р±С‹...");
    let _installing = crate::platform::InstallingService::new();
    let (_, _, _, exe) = get_install_info();
    let tmp_path = std::env::temp_dir().to_string_lossy().to_string();
    let tray_shortcut = get_tray_shortcut(&exe, &tmp_path).unwrap_or_default();
    let filter = format!(" /FI \"PID ne {}\"", get_current_pid());
    Config::set_option("stop-service".into(), "".into());
    crate::ipc::EXIT_RECV_CLOSE.store(false, Ordering::Relaxed);
    let cmds = format!(
        "
chcp 65001
taskkill /F /IM {app_name}.exe{filter}
cscript \"{tray_shortcut}\"
copy /Y \"{tmp_path}\\{app_name} Tray.lnk\" \"%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\\"
{import_config}
{create_service}
if exist \"{tray_shortcut}\" del /f /q \"{tray_shortcut}\"
    ",
        app_name = crate::get_app_name(),
        import_config = get_import_config(&exe),
        create_service = get_create_service(&exe),
    );
    if let Err(err) = run_cmds(cmds, false, "install") {
        Config::set_option("stop-service".into(), "Y".into());
        crate::ipc::EXIT_RECV_CLOSE.store(true, Ordering::Relaxed);
        log::debug!("{err}");
        return true;
    }
    run_after_run_cmds(false);
    std::process::exit(0);
}

pub fn update_me(debug: bool) -> ResultType<()> {
    let app_name = crate::get_app_name();
    let src_exe = std::env::current_exe()?.to_string_lossy().to_string();
    let (subkey, path, _, exe) = get_install_info();
    let is_installed = std::fs::metadata(&exe).is_ok();
    if !is_installed {
        bail!("{} РЅРµ СѓСЃС‚Р°РЅРѕРІР»РµРЅ.", &app_name);
    }

    let app_exe_name = &format!("{}.exe", &app_name);
    let main_window_pids =
        crate::platform::get_pids_of_process_with_args::<_, &str>(&app_exe_name, &[]);
    let main_window_sessions = main_window_pids
        .iter()
        .map(|pid| get_session_id_of_process(pid.as_u32()))
        .flatten()
        .collect::<Vec<_>>();
    kill_process_by_pids(&app_exe_name, main_window_pids)?;
    let tray_pids = crate::platform::get_pids_of_process_with_args(&app_exe_name, &["--tray"]);
    let tray_sessions = tray_pids
        .iter()
        .map(|pid| get_session_id_of_process(pid.as_u32()))
        .flatten()
        .collect::<Vec<_>>();
    kill_process_by_pids(&app_exe_name, tray_pids)?;
    let is_service_running = is_self_service_running();

    let mut version_major = "0";
    let mut version_minor = "0";
    let mut version_build = "0";
    let versions: Vec<&str> = crate::VERSION.split(".").collect();
    if versions.len() > 0 {
        version_major = versions[0];
    }
    if versions.len() > 1 {
        version_minor = versions[1];
    }
    if versions.len() > 2 {
        version_build = versions[2];
    }
    let meta = std::fs::symlink_metadata(std::env::current_exe()?)?;
    let size = meta.len() / 1024;

    let reg_cmd = format!(
        "
reg add {subkey} /f /v DisplayIcon /t REG_SZ /d \"{exe}\"
reg add {subkey} /f /v DisplayVersion /t REG_SZ /d \"{version}\"
reg add {subkey} /f /v Version /t REG_SZ /d \"{version}\"
reg add {subkey} /f /v BuildDate /t REG_SZ /d \"{build_date}\"
reg add {subkey} /f /v VersionMajor /t REG_DWORD /d {version_major}
reg add {subkey} /f /v VersionMinor /t REG_DWORD /d {version_minor}
reg add {subkey} /f /v VersionBuild /t REG_DWORD /d {version_build}
reg add {subkey} /f /v EstimatedSize /t REG_DWORD /d {size}
    ",
        version = crate::VERSION.replace("-", "."),
        build_date = crate::BUILD_DATE,
    );

    let filter = format!(" /FI \"PID ne {}\"", get_current_pid());
    let restore_service_cmd = if is_service_running {
        format!("sc start {}", &app_name)
    } else {
        "".to_owned()
    };

    // РќРµС‚ РЅРµРѕР±С…РѕРґРёРјРѕСЃС‚Рё РїСЂРѕРІРµСЂСЏС‚СЊ РїР°СЂР°РјРµС‚СЂ СѓСЃС‚Р°РЅРѕРІРєРё Р·РґРµСЃСЊ, `is_rd_printer_installed` СЂРµРґРєРѕ РїСЂРѕРІР°Р»РёРІР°РµС‚СЃСЏ.
    let is_printer_installed = remote_printer::is_rd_printer_installed(&app_name).unwrap_or(false);
    // РќРёС‡РµРіРѕ РЅРµ РґРµР»Р°С‚СЊ, РµСЃР»Рё РїСЂРёРЅС‚РµСЂ РЅРµ СѓСЃС‚Р°РЅРѕРІР»РµРЅ РёР»Рё РЅРµ СѓРґР°Р»РѕСЃСЊ Р·Р°РїСЂРѕСЃРёС‚СЊ, СѓСЃС‚Р°РЅРѕРІР»РµРЅ Р»Рё РїСЂРёРЅС‚РµСЂ.
    let (uninstall_printer_cmd, install_printer_cmd) = if is_printer_installed {
        (
            format!("\"{}\" --uninstall-remote-printer", &src_exe),
            format!("\"{}\" --install-remote-printer", &src_exe),
        )
    } else {
        ("".to_owned(), "".to_owned())
    };

    // РњС‹ РЅРµ РїС‹С‚Р°РµРјСЃСЏ СѓРґР°Р»РёС‚СЊ РІСЃРµ С„Р°Р№Р»С‹ РІ СЃС‚Р°СЂРѕР№ РІРµСЂСЃРёРё.
    // РџРѕС‚РѕРјСѓ С‡С‚Рѕ СЏ РЅРµ Р·РЅР°СЋ, Р±СѓРґСѓС‚ Р»Рё Р·РґРµСЃСЊ СѓСЃС‚Р°РЅРѕРІР»РµРЅС‹ РґРѕРїРѕР»РЅРёС‚РµР»СЊРЅС‹Рµ С„Р°Р№Р»С‹ РїРѕСЃР»Рµ СѓСЃС‚Р°РЅРѕРІРєРё, С‚Р°РєРёРµ РєР°Рє РґСЂР°Р№РІРµСЂС‹.
    // РџСЂРѕСЃС‚Рѕ РєРѕРїРёСЂРѕРІР°РЅРёРµ С„Р°Р№Р»РѕРІ РІ РєР°С‚Р°Р»РѕРі СѓСЃС‚Р°РЅРѕРІРєРё СЂР°Р±РѕС‚Р°РµС‚ РЅРѕСЂРјР°Р»СЊРЅРѕ.
    //if exist \"{path}\" rd /s /q \"{path}\"
    // md \"{path}\"
    //
    // РќР°Рј РЅСѓР¶РµРЅ `taskkill`, РїРѕС‚РѕРјСѓ С‡С‚Рѕ:
    // 1. РњРѕРіСѓС‚ СЂР°Р±РѕС‚Р°С‚СЊ РЅРµРєРѕС‚РѕСЂС‹Рµ РґСЂСѓРіРёРµ РїСЂРѕС†РµСЃСЃС‹, С‚Р°РєРёРµ РєР°Рє `rustdesk --connect`.
    // 2. РРЅРѕРіРґР° РіР»Р°РІРЅРѕРµ РѕРєРЅРѕ Рё Р·РЅР°С‡РѕРє РІ С‚СЂРµРµ РѕС‚РѕР±СЂР°Р¶Р°СЋС‚СЃСЏ
    // РІ С‚Рѕ РІСЂРµРјСЏ РєР°Рє СЏ РЅРµ РјРѕРіСѓ РЅР°Р№С‚Рё РёС… СЃ РїРѕРјРѕС‰СЊСЋ `tasklist` РёР»Рё РјРµС‚РѕРґРѕРІ РІС‹С€Рµ.
    // Р”РѕР»Р¶РЅРѕ СЂР°Р±РѕС‚Р°С‚СЊ 4 РїСЂРѕС†РµСЃСЃР°: СЃР»СѓР¶Р±Р°, СЃРµСЂРІРµСЂ, С‚СЂРµР№ Рё РіР»Р°РІРЅРѕРµ.
    // РќРѕ РІ tasklist РѕС‚РѕР±СЂР°Р¶Р°РµС‚СЃСЏ С‚РѕР»СЊРєРѕ 2 РїСЂРѕС†РµСЃСЃР°.
    let cmds = format!(
        "
chcp 65001
sc stop {app_name}
taskkill /F /IM {app_name}.exe{filter}
{reg_cmd}
{copy_exe}
{restore_service_cmd}
{uninstall_printer_cmd}
{install_printer_cmd}
{sleep}
    ",
        app_name = app_name,
        copy_exe = copy_exe_cmd(&src_exe, &exe, &path)?,
        sleep = if debug { "timeout 300" } else { "" },
    );

    run_cmds(cmds, debug, "update")?;

    std::thread::sleep(std::time::Duration::from_millis(2000));
    if tray_sessions.is_empty() {
        log::info!("Р—РЅР°С‡РѕРє РІ С‚СЂРµРµ РЅРµ РЅР°Р№РґРµРЅ.");
    } else {
        log::info!("РџРѕРїС‹С‚РєР° РІРѕСЃСЃС‚Р°РЅРѕРІРёС‚СЊ РїСЂРѕС†РµСЃСЃ С‚СЂРµСЏ...");
        log::info!(
            "РџРѕРїС‹С‚РєР° РІРѕСЃСЃС‚Р°РЅРѕРІРёС‚СЊ РїСЂРѕС†РµСЃСЃ С‚СЂРµСЏ..., СЃРµСЃСЃРёРё: {:?}",
            &tray_sessions
        );
        for s in tray_sessions {
            if s != 0 {
                allow_err!(run_exe_in_session(&exe, vec!["--tray"], s, true));
            }
        }
    }
    if main_window_sessions.is_empty() {
        log::info!("Р“Р»Р°РІРЅРѕРµ РѕРєРЅРѕ РЅРµ РЅР°Р№РґРµРЅРѕ.");
    } else {
        log::info!("РџРѕРїС‹С‚РєР° РІРѕСЃСЃС‚Р°РЅРѕРІРёС‚СЊ РїСЂРѕС†РµСЃСЃ РіР»Р°РІРЅРѕРіРѕ РѕРєРЅР°...");
        std::thread::sleep(std::time::Duration::from_millis(2000));
        for s in main_window_sessions {
            if s != 0 {
                allow_err!(run_exe_in_session(&exe, vec![], s, true));
            }
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    log::info!("РћР±РЅРѕРІР»РµРЅРёРµ Р·Р°РІРµСЂС€РµРЅРѕ.");

    Ok(())
}

// Р”РІРѕР№РЅРѕРµ РїРѕРґС‚РІРµСЂР¶РґРµРЅРёРµ РёРјРµРЅРё РїСЂРѕС†РµСЃСЃР°
fn kill_process_by_pids(name: &str, pids: Vec<Pid>) -> ResultType<()> {
    let name = name.to_lowercase();
    let s = System::new_all();
    // РќРµС‚ РЅРµРѕР±С…РѕРґРёРјРѕСЃС‚Рё СЃРЅР°С‡Р°Р»Р° РїСЂРѕРІРµСЂСЏС‚СЊ РІСЃРµ РёРјРµРЅР° `pids`, Р° Р·Р°С‚РµРј СѓР±РёРІР°С‚СЊ РёС….
    // Р­С‚Рѕ СЂРµРґРєРёР№ СЃР»СѓС‡Р°Р№, РєРѕРіРґР° РѕРЅРё РЅРµ СЃРѕРІРїР°РґР°СЋС‚.
    for pid in pids {
        if let Some(process) = s.process(pid) {
            if process.name().to_lowercase() != name {
                bail!("РќРµ СѓРґР°Р»РѕСЃСЊ СѓР±РёС‚СЊ РїСЂРѕС†РµСЃСЃ, РёРјРµРЅР° РЅРµ СЃРѕРІРїР°РґР°СЋС‚.");
            }
            if !process.kill() {
                bail!("РќРµ СѓРґР°Р»РѕСЃСЊ СѓР±РёС‚СЊ РїСЂРѕС†РµСЃСЃ");
            }
        } else {
            bail!("РќРµ СѓРґР°Р»РѕСЃСЊ СѓР±РёС‚СЊ РїСЂРѕС†РµСЃСЃ, pid РЅРµ РЅР°Р№РґРµРЅ");
        }
    }
    Ok(())
}

// РќРµ Р·Р°РїСѓСЃРєР°С‚СЊ РїСЂРёР»РѕР¶РµРЅРёРµ РІ С‚СЂРµРµ РїСЂРё Р·Р°РїСѓСЃРєРµ СЃ `\qn`.
// 1. РџРѕС‚РѕРјСѓ С‡С‚Рѕ `/qn` С‚СЂРµР±СѓРµС‚ РїСЂР°РІ Р°РґРјРёРЅРёСЃС‚СЂР°С‚РѕСЂР°, Рё РїСЂРёР»РѕР¶РµРЅРёРµ РІ С‚СЂРµРµ РґРѕР»Р¶РЅРѕ Р·Р°РїСѓСЃРєР°С‚СЊСЃСЏ СЃ РїСЂР°РІР°РјРё РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ.
//   РР»Рё Р·Р°РїСѓСЃРє РіР»Р°РІРЅРѕРіРѕ РѕРєРЅР° РёР· РїСЂРёР»РѕР¶РµРЅРёСЏ РІ С‚СЂРµРµ РїСЂРёРІРµРґРµС‚ Рє Р·Р°РїСѓСЃРєСѓ РіР»Р°РІРЅРѕРіРѕ РѕРєРЅР° СЃ РїСЂР°РІР°РјРё Р°РґРјРёРЅРёСЃС‚СЂР°С‚РѕСЂР°.
// 2. РњС‹ РЅРµ РјРѕР¶РµРј Р·Р°РїСѓСЃС‚РёС‚СЊ РїСЂРёР»РѕР¶РµРЅРёРµ РІ С‚СЂРµРµ, РµСЃР»Рё UI РЅР° СЌРєСЂР°РЅРµ РІС…РѕРґР°.
// `fn update_me()` РјРѕР¶РµС‚ РѕР±СЂР°Р±Р°С‚С‹РІР°С‚СЊ РІС‹С€РµСѓРєР°Р·Р°РЅРЅС‹Рµ СЃР»СѓС‡Р°Рё, РЅРѕ РґР»СЏ РѕР±РЅРѕРІР»РµРЅРёСЏ msi РЅР°Рј РЅСѓР¶РЅРѕ СЃРґРµР»Р°С‚СЊ Р±РѕР»СЊС€Рµ СЂР°Р±РѕС‚С‹ РґР»СЏ РѕР±СЂР°Р±РѕС‚РєРё РІС‹С€РµСѓРєР°Р·Р°РЅРЅС‹С… СЃР»СѓС‡Р°РµРІ.
//    1. Р—Р°РїРёСЃР°С‚СЊ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂС‹ СЃРµСЃСЃРёР№ РїСЂРёР»РѕР¶РµРЅРёСЏ РІ С‚СЂРµРµ.
//    2. Р’С‹РїРѕР»РЅРёС‚СЊ РѕР±РЅРѕРІР»РµРЅРёРµ.
//    3. Р’РѕСЃСЃС‚Р°РЅРѕРІРёС‚СЊ СЃРµСЃСЃРёРё РїСЂРёР»РѕР¶РµРЅРёСЏ РІ С‚СЂРµРµ.
//    `1` Рё `3` РґРѕР»Р¶РЅС‹ Р±С‹С‚СЊ СЃРґРµР»Р°РЅС‹ РІ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊСЃРєРёС… РґРµР№СЃС‚РІРёСЏС….
//    РќР°Рј С‚Р°РєР¶Рµ РЅСѓР¶РЅРѕ РѕР±СЂР°Р±РѕС‚Р°С‚СЊ СЂР°Р·Р±РѕСЂ РєРѕРјР°РЅРґРЅРѕР№ СЃС‚СЂРѕРєРё, С‡С‚РѕР±С‹ РЅР°Р№С‚Рё РїСЂРѕС†РµСЃСЃС‹ С‚СЂРµСЏ.
pub fn update_me_msi(msi: &str, quiet: bool) -> ResultType<()> {
    let cmds = format!(
        "chcp 65001 && msiexec /i {msi} {}",
        if quiet { "/qn LAUNCH_TRAY_APP=N" } else { "" }
    );
    run_cmds(cmds, false, "update-msi")?;
    Ok(())
}

pub fn get_tray_shortcut(exe: &str, tmp_path: &str) -> ResultType<String> {
    Ok(write_cmds(
        format!(
            "
Set oWS = WScript.CreateObject(\"WScript.Shell\")
sLinkFile = \"{tmp_path}\\{app_name} Tray.lnk\"

Set oLink = oWS.CreateShortcut(sLinkFile)
    oLink.TargetPath = \"{exe}\"
    oLink.Arguments = \"--tray\"
oLink.Save
        ",
            app_name = crate::get_app_name(),
        ),
        "vbs",
        "tray_shortcut",
    )?
    .to_str()
    .unwrap_or("")
    .to_owned())
}

fn get_import_config(exe: &str) -> String {
    if config::is_outgoing_only() {
        return "".to_string();
    }
    format!("
sc stop {app_name}
sc delete {app_name}
sc create {app_name} binpath= \"\\\"{exe}\\\" --import-config \\\"{config_path}\\\"\" start= auto DisplayName= \"{app_name} Service\"
sc start {app_name}
sc stop {app_name}
sc delete {app_name}
",
    app_name = crate::get_app_name(),
    config_path=Config::file().to_str().unwrap_or(""),
)
}

fn get_create_service(exe: &str) -> String {
    if config::is_outgoing_only() {
        return "".to_string();
    }
    let stop = Config::get_option("stop-service") == "Y";
    if stop {
        format!("
if exist \"%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\{app_name} Tray.lnk\" del /f /q \"%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\{app_name} Tray.lnk\"
", app_name = crate::get_app_name())
    } else {
        format!("
sc create {app_name} binpath= \"\\\"{exe}\\\" --service\" start= auto DisplayName= \"{app_name} Service\"
sc start {app_name}
",
    app_name = crate::get_app_name())
    }
}

fn run_after_run_cmds(silent: bool) {
    let (_, _, _, exe) = get_install_info();
    if !silent {
        log::debug!("РЎРѕР·РґР°С‚СЊ РЅРѕРІРѕРµ РѕРєРЅРѕ");
        allow_err!(std::process::Command::new("cmd")
            .args(&["/c", "timeout", "/t", "2", "&", &format!("{exe}")])
            .creation_flags(winapi::um::winbase::CREATE_NO_WINDOW)
            .spawn());
    }
    if Config::get_option("stop-service") != "Y" {
        allow_err!(std::process::Command::new(&exe).arg("--tray").spawn());
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
}

#[inline]
pub fn try_kill_broker() {
    allow_err!(std::process::Command::new("cmd")
        .arg("/c")
        .arg(&format!(
            "taskkill /F /IM {}",
            WIN_TOPMOST_INJECTED_PROCESS_EXE
        ))
        .creation_flags(winapi::um::winbase::CREATE_NO_WINDOW)
        .spawn());
}

pub fn message_box(text: &str) {
    let mut text = text.to_owned();
    let nodialog = std::env::var("NO_DIALOG").unwrap_or_default() == "Y";
    if !text.ends_with("!") || nodialog {
        use arboard::Clipboard as ClipboardContext;
        match ClipboardContext::new() {
            Ok(mut ctx) => {
                ctx.set_text(&text).ok();
                if !nodialog {
                    text = format!("{}\n\nР’С‹С€РµСѓРєР°Р·Р°РЅРЅС‹Р№ С‚РµРєСЃС‚ СЃРєРѕРїРёСЂРѕРІР°РЅ РІ Р±СѓС„РµСЂ РѕР±РјРµРЅР°", &text);
                }
            }
            _ => {}
        }
    }
    if nodialog {
        if std::env::var("PRINT_OUT").unwrap_or_default() == "Y" {
            println!("{text}");
        }
        if let Ok(x) = std::env::var("WRITE_TO_FILE") {
            if !x.is_empty() {
                allow_err!(std::fs::write(x, text));
            }
        }
        return;
    }
    let text = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    let caption = "Р’С‹РІРѕРґ RustDesk"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_OK) };
}

pub fn alloc_console() {
    unsafe {
        alloc_console_and_redirect();
    }
}

fn get_license() -> Option<CustomServer> {
    let mut lic: CustomServer = Default::default();
    if let Ok(tmp) = get_license_from_exe_name() {
        lic = tmp;
    } else {
        // РґР»СЏ РѕР±СЂР°С‚РЅРѕР№ СЃРѕРІРјРµСЃС‚РёРјРѕСЃС‚Рё РїСЂРё РјРёРіСЂР°С†РёРё СЃ <= 1.2.1 РЅР° 1.2.2
        lic.key = get_reg("Key");
        lic.host = get_reg("Host");
        lic.api = get_reg("Api");
    }
    if lic.key.is_empty() || lic.host.is_empty() {
        return None;
    }
    Some(lic)
}

pub struct WallPaperRemover {
    old_path: String,
}

impl WallPaperRemover {
    pub fn new() -> ResultType<Self> {
        let start = std::time::Instant::now();
        if !Self::need_remove() {
            bail!("СѓР¶Рµ СЃРїР»РѕС€РЅРѕР№ С†РІРµС‚");
        }
        let old_path = match Self::get_recent_wallpaper() {
            Ok(old_path) => old_path,
            Err(e) => {
                log::info!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РЅРµРґР°РІРЅРёРµ РѕР±РѕРё: {:?}, РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ СЂРµР·РµСЂРІРЅС‹Р№", e);
                wallpaper::get().map_err(|e| anyhow!(e.to_string()))?
            }
        };
        Self::set_wallpaper(None)?;
        log::info!(
            "СЃРѕР·РґР°РЅ СѓРґР°Р»РёС‚РµР»СЊ РѕР±РѕРµРІ,  old_path: {:?},  РїСЂРѕС€РµРґС€РµРµ РІСЂРµРјСЏ: {:?}",
            old_path,
            start.elapsed(),
        );
        Ok(Self { old_path })
    }

    pub fn support() -> bool {
        wallpaper::get().is_ok() || !Self::get_recent_wallpaper().unwrap_or_default().is_empty()
    }

    fn get_recent_wallpaper() -> ResultType<String> {
        // SystemParametersInfoW РјРѕР¶РµС‚ РІРµСЂРЅСѓС‚СЊ %appdata%\Microsoft\Windows\Themes\TranscodedWallpaper, РЅРµ СЂРµР°Р»СЊРЅС‹Р№ РїСѓС‚СЊ Рё РјРѕР¶РµС‚ РЅРµ Р±С‹С‚СЊ СЂРµР°Р»СЊРЅС‹Рј РєСЌС€РµРј
        // https://www.makeuseof.com/find-desktop-wallpapers-file-location-windows-11/
        // https://superuser.com/questions/1218413/write-to-current-users-registry-through-a-different-admin-account
        let (hkcu, sid) = if is_root() {
            let sid = get_current_process_session_id().ok_or(anyhow!("РЅРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ sid"))?;
            (RegKey::predef(HKEY_USERS as isize), format!("{}\\", sid))
        } else {
            (RegKey::predef(HKEY_CURRENT_USER as isize), "".to_string())
        };
        let explorer_key = hkcu.open_subkey_with_flags(
            &format!(
                "{}Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Wallpapers",
                sid
            ),
            KEY_READ,
        )?;
        Ok(explorer_key.get_value("BackgroundHistoryPath0")?)
    }

    fn need_remove() -> bool {
        if let Ok(wallpaper) = wallpaper::get() {
            return !wallpaper.is_empty();
        }
        false
    }

    fn set_wallpaper(path: Option<String>) -> ResultType<()> {
        wallpaper::set_from_path(&path.unwrap_or_default()).map_err(|e| anyhow!(e.to_string()))
    }
}

impl Drop for WallPaperRemover {
    fn drop(&mut self) {
        // Р•СЃР»Рё СЃС‚Р°СЂС‹Р№ С„РѕРЅ - СЃР»Р°Р№Рґ-С€РѕСѓ, РѕРЅ Р±СѓРґРµС‚ РїСЂРµРѕР±СЂР°Р·РѕРІР°РЅ РІ РёР·РѕР±СЂР°Р¶РµРЅРёРµ. AnyDesk РґРµР»Р°РµС‚ С‚Рѕ Р¶Рµ СЃР°РјРѕРµ.
        allow_err!(Self::set_wallpaper(Some(self.old_path.clone())));
    }
}

fn get_uninstall_amyuni_idd() -> String {
    match std::env::current_exe() {
        Ok(path) => format!("\"{}\" --uninstall-amyuni-idd", path.to_str().unwrap_or("")),
        Err(e) => {
            log::warn!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РїСѓС‚СЊ С‚РµРєСѓС‰РµРіРѕ exe, РЅРµ СѓРґР°РµС‚СЃСЏ РїРѕР»СѓС‡РёС‚СЊ РєРѕРјР°РЅРґСѓ РґРµРёРЅСЃС‚Р°Р»Р»СЏС†РёРё idd, РћС€РёР±РєР°: {:?}", e);
            "".to_string()
        }
    }
}

#[inline]
pub fn is_self_service_running() -> bool {
    is_service_running(&crate::get_app_name())
}

pub fn is_service_running(service_name: &str) -> bool {
    unsafe {
        let service_name = wide_string(service_name);
        is_service_running_w(service_name.as_ptr() as _)
    }
}

pub fn is_x64() -> bool {
    const PROCESSOR_ARCHITECTURE_AMD64: u16 = 9;

    let mut sys_info = SYSTEM_INFO::default();
    unsafe {
        GetNativeSystemInfo(&mut sys_info as _);
    }
    unsafe { sys_info.u.s().wProcessorArchitecture == PROCESSOR_ARCHITECTURE_AMD64 }
}

pub fn try_kill_rustdesk_main_window_process() -> ResultType<()> {
    // РЈР±РёС‚СЊ rustdesk.exe Р±РµР· РґРѕРї. Р°СЂРіСѓРјРµРЅС‚Р°, РґРѕР»Р¶РµРЅ РІС‹Р·С‹РІР°С‚СЊСЃСЏ С‚РѕР»СЊРєРѕ --server
    // РњС‹ РјРѕР¶РµРј РЅР°Р№С‚Рё С‚РѕС‡РЅС‹Р№ РїСЂРѕС†РµСЃСЃ, РєРѕС‚РѕСЂС‹Р№ Р·Р°РЅРёРјР°РµС‚ ipc, РїРѕРґСЂРѕР±РЅРµРµ СЃРј. https://github.com/winsiderss/systeminformer
    log::info!("РїРѕРїС‹С‚РєР° СѓР±РёС‚СЊ РїСЂРѕС†РµСЃСЃ РіР»Р°РІРЅРѕРіРѕ РѕРєРЅР° rustdesk");
    use hbb_common::sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes();
    let my_uid = sys
        .process((std::process::id() as usize).into())
        .map(|x| x.user_id())
        .unwrap_or_default();
    let my_pid = std::process::id();
    let app_name = crate::get_app_name().to_lowercase();
    if app_name.is_empty() {
        bail!("РёРјСЏ РїСЂРёР»РѕР¶РµРЅРёСЏ РїСѓСЃС‚РѕРµ");
    }
    for (_, p) in sys.processes().iter() {
        let p_name = p.name().to_lowercase();
        // РёРјСЏ СЂР°РІРЅРѕ
        if !(p_name == app_name || p_name == app_name.clone() + ".exe") {
            continue;
        }
        // Р°СЂРіСѓРјРµРЅС‚РѕРІ Р±РѕР»СЊС€Рµ 1
        if p.cmd().len() < 1 {
            continue;
        }
        // РїРµСЂРІС‹Р№ Р°СЂРіСѓРјРµРЅС‚ СЃРѕРґРµСЂР¶РёС‚ РёРјСЏ РїСЂРёР»РѕР¶РµРЅРёСЏ
        if !p.cmd()[0].to_lowercase().contains(&p_name) {
            continue;
        }
        // С‚РѕР»СЊРєРѕ РѕРґРёРЅ Р°СЂРіСѓРјРµРЅС‚ РёР»Рё РІС‚РѕСЂРѕР№ Р°СЂРіСѓРјРµРЅС‚ РїСѓСЃС‚Р°СЏ uni-СЃСЃС‹Р»РєР°
        let is_empty_uni = p.cmd().len() == 2 && crate::common::is_empty_uni_link(&p.cmd()[1]);
        if !(p.cmd().len() == 1 || is_empty_uni) {
            continue;
        }
        // РїСЂРѕРїСѓСЃС‚РёС‚СЊ СЃРµР±СЏ
        if p.pid().as_u32() == my_pid {
            continue;
        }
        // РїРѕС‚РѕРјСѓ С‡С‚Рѕ РјС‹ РІС‹Р·С‹РІР°РµРј СЌС‚Рѕ СЃ --server, С‚Р°Рє С‡С‚Рѕ РјРѕР¶РµРј РїСЂРѕРІРµСЂРёС‚СЊ user_id, СѓРґР°Р»РёС‚СЊ СЌС‚Рѕ, РµСЃР»Рё РІС‹Р·С‹РІР°С‚СЊ СЃ РїСЂРѕС†РµСЃСЃРѕРј РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ
        if p.user_id() == my_uid {
            log::info!("РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ СЂР°РІРµРЅ, РїСЂРѕРґРѕР»Р¶РёС‚СЊ");
            continue;
        }
        log::info!("РїРѕРїС‹С‚РєР° СѓР±РёС‚СЊ РїСЂРѕС†РµСЃСЃ: {:?}, pid = {:?}", p.cmd(), p.pid());
        nt_terminate_process(p.pid().as_u32())?;
        log::info!("СѓР±РёР№СЃС‚РІРѕ РїСЂРѕС†РµСЃСЃР° СѓСЃРїРµС€РЅРѕ: {:?}, pid = {:?}", p.cmd(), p.pid());
        return Ok(());
    }
    bail!("РЅРµ СѓРґР°Р»РѕСЃСЊ РЅР°Р№С‚Рё РїСЂРѕС†РµСЃСЃ РіР»Р°РІРЅРѕРіРѕ РѕРєРЅР° rustdesk");
}

fn nt_terminate_process(process_id: DWORD) -> ResultType<()> {
    type NtTerminateProcess = unsafe extern "system" fn(HANDLE, DWORD) -> DWORD;
    unsafe {
        let h_module = if is_win_10_or_greater() {
            LoadLibraryExA(
                CString::new("ntdll.dll")?.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        } else {
            LoadLibraryA(CString::new("ntdll.dll")?.as_ptr())
        };
        if !h_module.is_null() {
            let f_nt_terminate_process: NtTerminateProcess = std::mem::transmute(GetProcAddress(
                h_module,
                CString::new("NtTerminateProcess")?.as_ptr(),
            ));
            let h_token = OpenProcess(PROCESS_ALL_ACCESS, 0, process_id);
            if !h_token.is_null() {
                if f_nt_terminate_process(h_token, 1) == 0 {
                    log::info!("Р·Р°РІРµСЂС€РµРЅРёРµ РїСЂРѕС†РµСЃСЃР° {} СѓСЃРїРµС€РЅРѕ", process_id);
                    CloseHandle(h_token);
                    return Ok(());
                } else {
                    CloseHandle(h_token);
                    bail!("NtTerminateProcess {} РЅРµ СѓРґР°Р»Р°СЃСЊ", process_id);
                }
            } else {
                bail!("OpenProcess {} РЅРµ СѓРґР°Р»Р°СЃСЊ", process_id);
            }
        } else {
            bail!("РќРµ СѓРґР°Р»РѕСЃСЊ Р·Р°РіСЂСѓР·РёС‚СЊ ntdll.dll");
        }
    }
}

pub fn try_set_window_foreground(window: HWND) {
    let env_key = SET_FOREGROUND_WINDOW;
    if let Ok(value) = std::env::var(env_key) {
        if value == "1" {
            unsafe {
                SetForegroundWindow(window);
            }
            std::env::remove_var(env_key);
        }
    }
}

pub mod reg_display_settings {
    use hbb_common::ResultType;
    use serde_derive::{Deserialize, Serialize};
    use std::collections::HashMap;
    use winreg::{enums::*, RegValue};
    const REG_GRAPHICS_DRIVERS_PATH: &str = "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers";
    const REG_CONNECTIVITY_PATH: &str = "Connectivity";

    #[derive(Serialize, Deserialize, Debug)]
    pub struct RegRecovery {
        path: String,
        key: String,
        old: (Vec<u8>, isize),
        new: (Vec<u8>, isize),
    }

    pub fn read_reg_connectivity() -> ResultType<HashMap<String, HashMap<String, RegValue>>> {
        let hklm = winreg::RegKey::predef(HKEY_LOCAL_MACHINE as isize);
        let reg_connectivity = hklm.open_subkey_with_flags(
            format!("{}\\{}", REG_GRAPHICS_DRIVERS_PATH, REG_CONNECTIVITY_PATH),
            KEY_READ,
        )?;

        let mut map_connectivity = HashMap::new();
        for key in reg_connectivity.enum_keys() {
            let key = key?;
            let mut map_item = HashMap::new();
            let reg_item = reg_connectivity.open_subkey_with_flags(&key, KEY_READ)?;
            for value in reg_item.enum_values() {
                let (name, value) = value?;
                map_item.insert(name, value);
            }
            map_connectivity.insert(key, map_item);
        }
        Ok(map_connectivity)
    }

    pub fn diff_recent_connectivity(
        map1: HashMap<String, HashMap<String, RegValue>>,
        map2: HashMap<String, HashMap<String, RegValue>>,
    ) -> Option<RegRecovery> {
        for (subkey, map_item2) in map2 {
            if let Some(map_item1) = map1.get(&subkey) {
                let key = "Recent";
                if let Some(value1) = map_item1.get(key) {
                    if let Some(value2) = map_item2.get(key) {
                        if value1 != value2 {
                            return Some(RegRecovery {
                                path: format!(
                                    "{}\\{}\\{}",
                                    REG_GRAPHICS_DRIVERS_PATH, REG_CONNECTIVITY_PATH, subkey
                                ),
                                key: key.to_owned(),
                                old: (value1.bytes.clone(), value1.vtype.clone() as isize),
                                new: (value2.bytes.clone(), value2.vtype.clone() as isize),
                            });
                        }
                    }
                }
            }
        }
        None
    }

    pub fn restore_reg_connectivity(reg_recovery: RegRecovery, force: bool) -> ResultType<()> {
        let hklm = winreg::RegKey::predef(HKEY_LOCAL_MACHINE as isize);
        let reg_item = hklm.open_subkey_with_flags(&reg_recovery.path, KEY_READ | KEY_WRITE)?;
        if !force {
            let cur_reg_value = reg_item.get_raw_value(&reg_recovery.key)?;
            let new_reg_value = RegValue {
                bytes: reg_recovery.new.0,
                vtype: isize_to_reg_type(reg_recovery.new.1),
            };
            // РЎСЂР°РІРЅРёС‚СЊ, СЂР°РІРЅРѕ Р»Рё С‚РµРєСѓС‰РµРµ Р·РЅР°С‡РµРЅРёРµ РЅРѕРІРѕРјСѓ Р·РЅР°С‡РµРЅРёСЋ.
            // Р•СЃР»Рё РѕРЅРё РЅРµ СЂР°РІРЅС‹, Р·РЅР°С‡РµРЅРёРµ СЂРµРµСЃС‚СЂР° Р±С‹Р»Рѕ РёР·РјРµРЅРµРЅРѕ РґСЂСѓРіРёРјРё РїСЂРѕС†РµСЃСЃР°РјРё.
            // РўР°Рє С‡С‚Рѕ РјС‹ РЅРµ РІРѕСЃСЃС‚Р°РЅР°РІР»РёРІР°РµРј Р·РЅР°С‡РµРЅРёРµ СЂРµРµСЃС‚СЂР°.
            if cur_reg_value != new_reg_value {
                return Ok(());
            }
        }
        let reg_value = RegValue {
            bytes: reg_recovery.old.0,
            vtype: isize_to_reg_type(reg_recovery.old.1),
        };
        reg_item.set_raw_value(&reg_recovery.key, &reg_value)?;
        Ok(())
    }

    #[inline]
    fn isize_to_reg_type(i: isize) -> RegType {
        match i {
            0 => RegType::REG_NONE,
            1 => RegType::REG_SZ,
            2 => RegType::REG_EXPAND_SZ,
            3 => RegType::REG_BINARY,
            4 => RegType::REG_DWORD,
            5 => RegType::REG_DWORD_BIG_ENDIAN,
            6 => RegType::REG_LINK,
            7 => RegType::REG_MULTI_SZ,
            8 => RegType::REG_RESOURCE_LIST,
            9 => RegType::REG_FULL_RESOURCE_DESCRIPTOR,
            10 => RegType::REG_RESOURCE_REQUIREMENTS_LIST,
            11 => RegType::REG_QWORD,
            _ => RegType::REG_NONE,
        }
    }
}

pub fn get_printer_names() -> ResultType<Vec<String>> {
    let mut needed_bytes = 0;
    let mut returned_count = 0;

    unsafe {
        // РџРµСЂРІС‹Р№ РІС‹Р·РѕРІ РґР»СЏ РїРѕР»СѓС‡РµРЅРёСЏ С‚СЂРµР±СѓРµРјРѕРіРѕ СЂР°Р·РјРµСЂР° Р±СѓС„РµСЂР°
        EnumPrintersW(
            PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS,
            std::ptr::null_mut(),
            1,
            std::ptr::null_mut(),
            0,
            &mut needed_bytes,
            &mut returned_count,
        );

        let mut buffer = vec![0u8; needed_bytes as usize];

        if EnumPrintersW(
            PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS,
            std::ptr::null_mut(),
            1,
            buffer.as_mut_ptr() as *mut _,
            needed_bytes,
            &mut needed_bytes,
            &mut returned_count,
        ) == 0
        {
            return Err(anyhow!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРµСЂРµС‡РёСЃР»РёС‚СЊ РїСЂРёРЅС‚РµСЂС‹"));
        }

        let ptr = buffer.as_ptr() as *const PRINTER_INFO_1W;
        let printers = std::slice::from_raw_parts(ptr, returned_count as usize);

        Ok(printers
            .iter()
            .filter_map(|p| {
                let name = p.pName;
                if !name.is_null() {
                    let mut len = 0;
                    while len < 500 {
                        if name.add(len).is_null() || *name.add(len) == 0 {
                            break;
                        }
                        len += 1;
                    }
                    if len > 0 && len < 500 {
                        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
                            name, len,
                        )))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect())
    }
}

extern "C" {
    fn PrintXPSRawData(printer_name: *const u16, raw_data: *const u8, data_size: c_ulong) -> DWORD;
}

pub fn send_raw_data_to_printer(printer_name: Option<String>, data: Vec<u8>) -> ResultType<()> {
    let mut printer_name = printer_name.unwrap_or_default();
    if printer_name.is_empty() {
        // РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ GetDefaultPrinter РґР»СЏ РїРѕР»СѓС‡РµРЅРёСЏ РёРјРµРЅРё РїСЂРёРЅС‚РµСЂР° РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ
        let mut needed_bytes = 0;
        unsafe {
            GetDefaultPrinterW(std::ptr::null_mut(), &mut needed_bytes);
        }
        if needed_bytes > 0 {
            let mut default_printer_name = vec![0u16; needed_bytes as usize];
            unsafe {
                GetDefaultPrinterW(
                    default_printer_name.as_mut_ptr() as *mut _,
                    &mut needed_bytes,
                );
            }
            printer_name = String::from_utf16_lossy(&default_printer_name[..needed_bytes as usize]);
        }
    } else {
        if let Ok(names) = crate::platform::windows::get_printer_names() {
            if !names.contains(&printer_name) {
                // РќРµ СѓСЃС‚Р°РЅР°РІР»РёРІР°С‚СЊ РїРµСЂРІС‹Р№ РїСЂРёРЅС‚РµСЂ РєР°Рє С‚РµРєСѓС‰РёР№ РїСЂРёРЅС‚РµСЂ.
                // РћРЅ РјРѕР¶РµС‚ РЅРµ Р±С‹С‚СЊ Р¶РµР»Р°РµРјС‹Рј РїСЂРёРЅС‚РµСЂРѕРј.
                bail!("РРјСЏ РїСЂРёРЅС‚РµСЂР° \"{}\" РЅРµ РЅР°Р№РґРµРЅРѕ", &printer_name);
            }
        }
    }
    if printer_name.is_empty() {
        return Err(anyhow!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РёРјСЏ РїСЂРёРЅС‚РµСЂР°"));
    }

    log::info!("РћС‚РїСЂР°РІРєР° РґР°РЅРЅС‹С… РЅР° РїСЂРёРЅС‚РµСЂ: {}", &printer_name);
    let printer_name = wide_string(&printer_name);
    unsafe {
        let res = PrintXPSRawData(
            printer_name.as_ptr(),
            data.as_ptr() as *const u8,
            data.len() as c_ulong,
        );
        if res != 0 {
            bail!("РќРµ СѓРґР°Р»РѕСЃСЊ РѕС‚РїСЂР°РІРёС‚СЊ РґР°РЅРЅС‹Рµ РЅР° РїСЂРёРЅС‚РµСЂ, РїРѕРґСЂРѕР±РЅРѕСЃС‚Рё СЃРјРѕС‚СЂРёС‚Рµ РІ Р»РѕРіР°С… C:\\Windows\\temp\\test_rustdesk.log.");
        } else {
            log::info!("Р”Р°РЅРЅС‹Рµ СѓСЃРїРµС€РЅРѕ РѕС‚РїСЂР°РІР»РµРЅС‹ РЅР° РїСЂРёРЅС‚РµСЂ");
        }
    }

    Ok(())
}

fn get_pids<S: AsRef<str>>(name: S) -> ResultType<Vec<u32>> {
    let name = name.as_ref().to_lowercase();
    let mut pids = Vec::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        if snapshot == WinHANDLE::default() {
            return Ok(pids);
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let proc_name = OsString::from_wide(&entry.szExeFile)
                    .to_string_lossy()
                    .to_lowercase();

                if proc_name.contains(&name) {
                    pids.push(entry.th32ProcessID);
                }

                if !Process32NextW(snapshot, &mut entry).is_ok() {
                    break;
                }
            }
        }

        let _ = WinCloseHandle(snapshot);
    }

    Ok(pids)
}

pub fn is_msi_installed() -> std::io::Result<bool> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE as isize);
    let uninstall_key = hklm.open_subkey(format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}",
        crate::get_app_name()
    ))?;
    Ok(1 == uninstall_key.get_value::<u32, _>("WindowsInstaller")?)
}

pub fn is_cur_exe_the_installed() -> bool {
    let (_, _, _, exe) = get_install_info();
    // РџСЂРѕРІРµСЂРёС‚СЊ, СѓСЃС‚Р°РЅРѕРІР»РµРЅ Р»Рё, РїРѕС‚РѕРјСѓ С‡С‚Рѕ `exe` - РїСѓС‚СЊ РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ, РµСЃР»Рё РЅРµ СѓСЃС‚Р°РЅРѕРІР»РµРЅ.
    if !std::fs::metadata(&exe).is_ok() {
        return false;
    }
    let mut path = std::env::current_exe().unwrap_or_default();
    if let Ok(linked) = path.read_link() {
        path = linked;
    }
    let path = path.to_string_lossy().to_lowercase();
    path == exe.to_lowercase()
}

#[cfg(not(target_pointer_width = "64"))]
pub fn get_pids_with_first_arg_check_session<S1: AsRef<str>, S2: AsRef<str>>(
    name: S1,
    arg: S2,
    same_session_id: bool,
) -> ResultType<Vec<hbb_common::sysinfo::Pid>> {
    // РҐРѕС‚СЏ `wmic` РјРѕР¶РµС‚ РІРµСЂРЅСѓС‚СЊ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ СЃРµСЃСЃРёРё, РґР»СЏ РїСЂРѕСЃС‚РѕС‚С‹ РјС‹ РІРѕР·РІСЂР°С‰Р°РµРј С‚РѕР»СЊРєРѕ processid.
    let pids = get_pids_with_first_arg_by_wmic(name, arg);
    if !same_session_id {
        return Ok(pids);
    }
    let Some(cur_sid) = get_current_process_session_id() else {
        bail!("РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ СЃРµСЃСЃРёРё С‚РµРєСѓС‰РµРіРѕ РїСЂРѕС†РµСЃСЃР°");
    };
    let mut same_session_pids = vec![];
    for pid in pids.into_iter() {
        let mut sid = 0;
        if unsafe { ProcessIdToSessionId(pid.as_u32(), &mut sid) == TRUE } {
            if sid == cur_sid {
                same_session_pids.push(pid);
            }
        } else {
            // РўРѕР»СЊРєРѕ Р»РѕРіРёСЂРѕРІР°С‚СЊ Р·РґРµСЃСЊ, РїРѕС‚РѕРјСѓ С‡С‚Рѕ СЌС‚РѕС‚ РІС‹Р·РѕРІ РїРѕС‡С‚Рё РЅРёРєРѕРіРґР° РЅРµ РїСЂРѕРІР°Р»РёРІР°РµС‚СЃСЏ.
            log::warn!(
                "РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕР»СѓС‡РёС‚СЊ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ СЃРµСЃСЃРёРё РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂР° РїСЂРѕС†РµСЃСЃР°, РѕС€РёР±РєР°: {:?}",
                std::io::Error::last_os_error()
            );
        }
    }
    Ok(same_session_pids)
}

#[cfg(not(target_pointer_width = "64"))]
fn get_pids_with_args_from_wmic_output<S2: AsRef<str>>(
    output: std::borrow::Cow<'_, str>,
    name: &str,
    args: &[S2],
) -> Vec<hbb_common::sysinfo::Pid> {
    // CommandLine=
    // ProcessId=33796
    //
    // CommandLine=
    // ProcessId=34668
    //
    // CommandLine="C:\Program Files\RustDesk\RustDesk.exe" --tray
    // ProcessId=13728
    //
    // CommandLine="C:\Program Files\RustDesk\RustDesk.exe"
    // ProcessId=10136
    let mut pids = Vec::new();
    let mut proc_found = false;
    for line in output.lines() {
        if line.starts_with("ProcessId=") {
            if proc_found {
                if let Ok(pid) = line["ProcessId=".len()..].trim().parse::<u32>() {
                    pids.push(hbb_common::sysinfo::Pid::from_u32(pid));
                }
                proc_found = false;
            }
        } else if line.starts_with("CommandLine=") {
            proc_found = false;
            let cmd = line["CommandLine=".len()..].trim().to_lowercase();
            if args.is_empty() {
                if cmd.ends_with(&name) || cmd.ends_with(&format!("{}\"", &name)) {
                    proc_found = true;
                }
            } else {
                proc_found = args.iter().all(|arg| cmd.contains(arg.as_ref()));
            }
        }
    }
    pids
}

// РџСЂРёРјРµС‡Р°РЅРёРµ: Р°СЂРіСѓРјРµРЅС‚С‹ РЅРµ СЃСЂР°РІРЅРёРІР°СЋС‚СЃСЏ СЃС‚СЂРѕРіРѕ, С‚РѕР»СЊРєРѕ РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ, СЃРѕРґРµСЂР¶Р°С‚СЃСЏ Р»Рё Р°СЂРіСѓРјРµРЅС‚С‹ РІ РєРѕРјР°РЅРґРЅРѕР№ СЃС‚СЂРѕРєРµ.
// Р•СЃР»Рё РјС‹ С…РѕС‚РёРј СЃС‚СЂРѕРіРѕ РїСЂРѕРІРµСЂРёС‚СЊ Р°СЂРіСѓРјРµРЅС‚С‹, РЅР°Рј РЅСѓР¶РЅРѕ СЂР°Р·РѕР±СЂР°С‚СЊ РєРѕРјР°РЅРґРЅСѓСЋ СЃС‚СЂРѕРєСѓ Рё СЃСЂР°РІРЅРёС‚СЊ РєР°Р¶РґС‹Р№ Р°СЂРіСѓРјРµРЅС‚.
// Р’РѕР·РјРѕР¶РЅРѕ, РЅР°Рј РЅСѓР¶РЅРѕ РІРІРµСЃС‚Рё РІРЅРµС€РЅРёР№ crate, С‚Р°РєРѕР№ РєР°Рє `shell_words`, С‡С‚РѕР±С‹ СЃРґРµР»Р°С‚СЊ СЌС‚Рѕ.
#[cfg(not(target_pointer_width = "64"))]
pub(super) fn get_pids_with_args_by_wmic<S1: AsRef<str>, S2: AsRef<str>>(
    name: S1,
    args: &[S2],
) -> Vec<hbb_common::sysinfo::Pid> {
    let name = name.as_ref().to_lowercase();
    std::process::Command::new("wmic.exe")
        .args([
            "process",
            "where",
            &format!("name='{}'", name),
            "get",
            "commandline,processid",
            "/value",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|output| {
            get_pids_with_args_from_wmic_output::<S2>(
                String::from_utf8_lossy(&output.stdout),
                &name,
                args,
            )
        })
        .unwrap_or_default()
}

#[cfg(not(target_pointer_width = "64"))]
fn get_pids_with_first_arg_from_wmic_output(
    output: std::borrow::Cow<'_, str>,
    name: &str,
    arg: &str,
) -> Vec<hbb_common::sysinfo::Pid> {
    let mut pids = Vec::new();
    let mut proc_found = false;
    for line in output.lines() {
        if line.starts_with("ProcessId=") {
            if proc_found {
                if let Ok(pid) = line["ProcessId=".len()..].trim().parse::<u32>() {
                    pids.push(hbb_common::sysinfo::Pid::from_u32(pid));
                }
                proc_found = false;
            }
        } else if line.starts_with("CommandLine=") {
            proc_found = false;
            let cmd = line["CommandLine=".len()..].trim().to_lowercase();
            if cmd.is_empty() {
                continue;
            }
            if !arg.is_empty() && cmd.starts_with(arg) {
                proc_found = true;
            } else {
                for x in [&format!("{}\"", name), &format!("{}", name)] {
                    if cmd.contains(x) {
                        let cmd = cmd.split(x).collect::<Vec<_>>()[1..].join("");
                        if arg.is_empty() {
                            if cmd.trim().is_empty() {
                                proc_found = true;
                            }
                        } else if cmd.trim().starts_with(arg) {
                            proc_found = true;
                        }
                        break;
                    }
                }
            }
        }
    }
    pids
}

// РџСЂРёРјРµС‡Р°РЅРёРµ: Р°СЂРіСѓРјРµРЅС‚С‹ РЅРµ СЃСЂР°РІРЅРёРІР°СЋС‚СЃСЏ СЃС‚СЂРѕРіРѕ, С‚РѕР»СЊРєРѕ РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ, СЃРѕРґРµСЂР¶Р°С‚СЃСЏ Р»Рё Р°СЂРіСѓРјРµРЅС‚С‹ РІ РєРѕРјР°РЅРґРЅРѕР№ СЃС‚СЂРѕРєРµ.
// Р•СЃР»Рё РјС‹ С…РѕС‚РёРј СЃС‚СЂРѕРіРѕ РїСЂРѕРІРµСЂРёС‚СЊ Р°СЂРіСѓРјРµРЅС‚С‹, РЅР°Рј РЅСѓР¶РЅРѕ СЂР°Р·РѕР±СЂР°С‚СЊ РєРѕРјР°РЅРґРЅСѓСЋ СЃС‚СЂРѕРєСѓ Рё СЃСЂР°РІРЅРёС‚СЊ РєР°Р¶РґС‹Р№ Р°СЂРіСѓРјРµРЅС‚.
// Р’РѕР·РјРѕР¶РЅРѕ, РЅР°Рј РЅСѓР¶РЅРѕ РІРІРµСЃС‚Рё РІРЅРµС€РЅРёР№ crate, С‚Р°РєРѕР№ РєР°Рє `shell_words`, С‡С‚РѕР±С‹ СЃРґРµР»Р°С‚СЊ СЌС‚Рѕ.
#[cfg(not(target_pointer_width = "64"))]
pub(super) fn get_pids_with_first_arg_by_wmic<S1: AsRef<str>, S2: AsRef<str>>(
    name: S1,
    arg: S2,
) -> Vec<hbb_common::sysinfo::Pid> {
    let name = name.as_ref().to_lowercase();
    let arg = arg.as_ref().to_lowercase();
    std::process::Command::new("wmic.exe")
        .args([
            "process",
            "where",
            &format!("name='{}'", name),
            "get",
            "commandline,processid",
            "/value",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|output| {
            get_pids_with_first_arg_from_wmic_output(
                String::from_utf8_lossy(&output.stdout),
                &name,
                &arg,
            )
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_uninstall_cert() {
        println!("РґРµРёРЅСЃС‚Р°Р»Р»СЏС†РёСЏ СЃРµСЂС‚РёС„РёРєР°С‚РѕРІ РґСЂР°Р№РІРµСЂР°: {:?}", cert::uninstall_cert());
    }

    #[test]
    fn test_get_unicode_char_by_vk() {
        let chr = get_char_from_vk(0x41); // VK_A
        assert_eq!(chr, Some('a'));
        let chr = get_char_from_vk(VK_ESCAPE as u32); // VK_ESC
        assert_eq!(chr, None)
    }

    #[cfg(not(target_pointer_width = "64"))]
    #[test]
    fn test_get_pids_with_args_from_wmic_output() {
        let output = r#"
CommandLine=
ProcessId=33796

CommandLine=
ProcessId=34668

CommandLine="C:\Program Files\testapp\TestApp.exe" --tray
ProcessId=13728

CommandLine="C:\Program Files\testapp\TestApp.exe"
ProcessId=10136
"#;
        let name = "testapp.exe";
        let args = vec!["--tray"];
        let pids = super::get_pids_with_args_from_wmic_output(
            String::from_utf8_lossy(output.as_bytes()),
            name,
            &args,
        );
        assert_eq!(pids.len(), 1);
        assert_eq!(pids[0].as_u32(), 13728);

        let args: Vec<&str> = vec![];
        let pids = super::get_pids_with_args_from_wmic_output(
            String::from_utf8_lossy(output.as_bytes()),
            name,
            &args,
        );
        assert_eq!(pids.len(), 1);
        assert_eq!(pids[0].as_u32(), 10136);

        let args = vec!["--other"];
        let pids = super::get_pids_with_args_from_wmic_output(
            String::from_utf8_lossy(output.as_bytes()),
            name,
            &args,
        );
        assert_eq!(pids.len(), 0);
    }

    #[cfg(not(target_pointer_width = "64"))]
    #[test]
    fn test_get_pids_with_first_arg_from_wmic_output() {
        let output = r#"
CommandLine=
ProcessId=33796

CommandLine=
ProcessId=34668

CommandLine="C:\Program Files\testapp\TestApp.exe" --tray
ProcessId=13728

CommandLine="C:\Program Files\testapp\TestApp.exe"
ProcessId=10136
    "#;
        let name = "testapp.exe";
        let arg = "--tray";
        let pids = super::get_pids_with_first_arg_from_wmic_output(
            String::from_utf8_lossy(output.as_bytes()),
            name,
            arg,
        );
        assert_eq!(pids.len(), 1);
        assert_eq!(pids[0].as_u32(), 13728);

        let arg = "";
        let pids = super::get_pids_with_first_arg_from_wmic_output(
            String::from_utf8_lossy(output.as_bytes()),
            name,
            arg,
        );
        assert_eq!(pids.len(), 1);
        assert_eq!(pids[0].as_u32(), 10136);

        let arg = "--other";
        let pids = super::get_pids_with_first_arg_from_wmic_output(
            String::from_utf8_lossy(output.as_bytes()),
            name,
            arg,
        );
        assert_eq!(pids.len(), 0);
    }
}
// ====== STUBS to replace missing `sas` and other native bindings ======
#[allow(non_snake_case, unused_variables, clippy::missing_safety_doc)]

#[cfg(target_os = "windows")]
mod native_stubs {
    use std::ptr;
    use std::ffi::c_void;
    use std::os::raw::c_int;

    // Р±РµСЂС‘Рј С‚РёРїС‹ РёР· winapi, С‚.Рє. РѕРЅРё СѓР¶Рµ РёСЃРїРѕР»СЊР·СѓСЋС‚СЃСЏ РІ С„Р°Р№Р»Рµ
    use winapi::shared::minwindef::{BOOL, DWORD, FALSE, TRUE};
    use winapi::shared::windef::{HBITMAP, HDC, HWND};
    use winapi::um::winnt::HANDLE;
use winapi::shared::minwindef::LPHANDLE;

    // ---------- С„СѓРЅРєС†РёРё РёР· РІР°С€РµРіРѕ extern "C" Р±Р»РѕРєР° ----------
    #[no_mangle]
    pub extern "C" fn get_current_session(rdp: BOOL) -> DWORD {
        // 0xFFFFFFFF Сѓ РІР°СЃ РґР°Р»РµРµ С‚СЂР°РєС‚СѓРµС‚СЃСЏ РєР°Рє В«РѕС€РёР±РєР°/РЅРµ СѓРґР°Р»РѕСЃСЊВ»
        0xFFFFFFFF
    }

    #[no_mangle]
    pub extern "C" fn LaunchProcessWin(
        cmd: *const u16,
        session_id: DWORD,
        as_user: BOOL,
        show: BOOL,
        token_pid: &mut DWORD,
    ) -> HANDLE {
        // СЃРѕРѕР±С‰РёРј РІС‹Р·С‹РІР°СЋС‰РµРјСѓ, С‡С‚Рѕ В«РЅРµ РЅР°С€Р»Рё winlogon/explorerВ» (РєР°Рє РІС‹ Р»РѕРіРёСЂСѓРµС‚Рµ РїСЂРё token_pid==0)
        *token_pid = 0;
        ptr::null_mut()
    }

    #[no_mangle]
    pub extern "C" fn GetSessionUserTokenWin(
        lphUserToken: LPHANDLE,
        dwSessionId: DWORD,
        as_user: BOOL,
        token_pid: &mut DWORD,
    ) -> BOOL {
        if !lphUserToken.is_null() {
            unsafe { *lphUserToken = ptr::null_mut() };
        }
        *token_pid = 0;
        FALSE
    }

    #[no_mangle]
    pub extern "C" fn selectInputDesktop() -> BOOL {
        FALSE
    }

    #[no_mangle]
    pub extern "C" fn inputDesktopSelected() -> BOOL {
        // FALSE Сѓ РІР°СЃ С‚СЂР°РєС‚СѓРµС‚СЃСЏ РєР°Рє В«РЅСѓР¶РЅРѕ РїРµСЂРµРєР»СЋС‡РёС‚СЊВ»
        FALSE
    }

    #[no_mangle]
    pub extern "C" fn is_windows_server() -> BOOL {
        FALSE
    }

    #[no_mangle]
    pub extern "C" fn is_windows_10_or_greater() -> BOOL {
        TRUE // РїСѓСЃРєР°Р№ РІРµРґС‘С‚ СЃРµР±СЏ РєР°Рє Win10+, СЌС‚Рѕ Р±РµР·РѕРїР°СЃРЅРµРµ РґР»СЏ РІРµС‚РѕРє РєРѕРґР°
    }

    #[no_mangle]
    pub extern "C" fn handleMask(
        out: *mut u8,
        mask: *const u8,
        width: i32,
        height: i32,
        bmWidthBytes: i32,
        bmHeight: i32,
    ) -> i32 {
        // В«0В» Сѓ РІР°СЃ С‚СЂР°РєС‚СѓРµС‚СЃСЏ РєР°Рє В«Р±РµР· РѕР±РІРѕРґРєРёВ»; >0 вЂ” Р±С‹Р»Р° РѕР±СЂР°Р±РѕС‚РєР°
        0
    }

    #[no_mangle]
    pub extern "C" fn drawOutline(
        out: *mut u8,
        in_: *const u8,
        width: i32,
        height: i32,
        out_size: i32,
    ) {
        // no-op
    }

    #[no_mangle]
    pub extern "C" fn get_di_bits(
        out: *mut u8,
        dc: HDC,
        hbmColor: HBITMAP,
        width: i32,
        height: i32,
    ) -> i32 {
        // Р’ РІР°С€РµРј РєРѕРґРµ: if get_di_bits(...) > 0 { bail!() }
        // Р’РµСЂРЅС‘Рј 0 (СѓСЃРїРµС…) Рё РЅРёС‡РµРіРѕ РЅРµ Р·Р°РїРѕР»РЅРёРј вЂ” РєСѓСЂСЃРѕСЂС‹ РїСЂРѕСЃС‚Рѕ Р±СѓРґСѓС‚ РїСѓСЃС‚С‹РјРё
        0
    }

    #[no_mangle]
    pub extern "C" fn blank_screen(v: BOOL) {
        // no-op
    }

    #[no_mangle]
    pub extern "C" fn win32_enable_lowlevel_keyboard(_hwnd: HWND) -> c_int {
        // 0 => СѓСЃРїРµС… Сѓ РІР°СЃ
        0
    }

    #[no_mangle]
    pub extern "C" fn win32_disable_lowlevel_keyboard(_hwnd: HWND) {
        // no-op
    }

    #[no_mangle]
    pub extern "C" fn win_stop_system_key_propagate(_v: BOOL) {
        // no-op
    }

    #[no_mangle]
    pub extern "C" fn is_win_down() -> BOOL {
        FALSE
    }

    #[no_mangle]
    pub extern "C" fn is_local_system() -> BOOL {
        // Р•СЃР»Рё С…РѕС‚РёС‚Рµ Р·Р°СЃС‚Р°РІРёС‚СЊ is_root() РІРµСЃС‚Рё СЃРµР±СЏ РєР°Рє SYSTEM вЂ” РІРµСЂРЅРёС‚Рµ TRUE.
        // РџРѕ СѓРјРѕР»С‡Р°РЅРёСЋ Р±РµР·РѕРїР°СЃРЅРµРµ FALSE, С‡С‚РѕР±С‹ РЅРµ СЌРјСѓР»РёСЂРѕРІР°С‚СЊ SYSTEM.
        FALSE
    }

    #[no_mangle]
    pub extern "C" fn alloc_console_and_redirect() {
        // no-op
    }

    #[no_mangle]
    pub extern "C" fn is_service_running_w(_svc_name: *const u16) -> bool {
        false
    }

    // ---------- РїСЂРѕС‡РёРµ C-СЃРёРјРІРѕР»С‹, РєРѕС‚РѕСЂС‹Рµ РґРµСЂРіР°СЋС‚СЃСЏ РїРѕР·РґРЅРµРµ ----------

    // РџРµС‡Р°С‚СЊ XPS В«СЃС‹СЂС‹С…В» РґР°РЅРЅС‹С…
    #[no_mangle]
    pub extern "C" fn PrintXPSRawData(
        _printer_name: *const u16,
        _raw_data: *const u8,
        _data_size: u32, // c_ulong -> u32
    ) -> DWORD {
        // Р’РµСЂРЅС‘Рј ERROR_CALL_NOT_IMPLEMENTED (120)
        120
    }

    // SAS (Secure Attention Sequence). Р’ РёСЃС…РѕРґРЅРёРєР°С… Р±С‹Р»Рѕ #[link(name="sas")] SendSAS
    // РћРїСЂРµРґРµР»РёРј Р»РѕРєР°Р»СЊРЅРѕ, С‡С‚РѕР±С‹ РЅРµ С‚СЂРµР±РѕРІР°Р»Р°СЃСЊ РІРЅРµС€РЅСЏСЏ sas.dll
    #[no_mangle]
    pub extern "system" fn SendSAS(_AsUser: BOOL) {
        // no-op
    }

    // Р”Р»СЏ AddRecentDocument РёР· РґСЂСѓРіРѕРіРѕ РјРµСЃС‚Р° С„Р°Р№Р»Р°
    #[no_mangle]
    pub extern "C" fn AddRecentDocument(_path: *const u16) {
        // no-op
    }

    // Р”Р»СЏ СѓРґР°Р»РµРЅРёСЏ С‚РµСЃС‚РѕРІС‹С… СЃРµСЂС‚РёС„РёРєР°С‚РѕРІ РґСЂР°Р№РІРµСЂР°
    #[no_mangle]
    pub extern "C" fn DeleteRustDeskTestCertsW() {
        // no-op
    }

    // РЎРїРёСЃРѕРє РґРѕСЃС‚СѓРїРЅС‹С… СЃРµСЃСЃРёР№ (СЃС‚СЂРѕРєРѕР№ В«Console:1,RDP-Tcp:2,...В» РІ Р±СѓС„РµСЂ wchar_t)
    #[no_mangle]
    pub extern "C" fn get_available_session_ids(
        buf: *mut u16,
        buf_size: c_int,
        _include_rdp: bool,
    ) {
        unsafe {
            if buf.is_null() || buf_size <= 0 {
                return;
            }
            // РџСѓСЃС‚Рѕ => РІС‹Р·С‹РІР°СЋС‰Р°СЏ СЃС‚РѕСЂРѕРЅР° РѕР±СЂР°Р±РѕС‚Р°РµС‚ gracefully
            *buf = 0;
        }
    }

    // РРјСЏ Р°РєС‚РёРІРЅРѕРіРѕ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ (РёСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РІ get_active_username)
    #[no_mangle]
    pub extern "C" fn get_active_user(path: *mut u16, n: u32, _rdp: BOOL) -> u32 {
        unsafe {
            if !path.is_null() && n > 0 {
                *path = 0;
            }
        }
        0 // 0 = В«РЅРёС‡РµРіРѕ РЅРµ РІРµСЂРЅСѓР»РёВ»
    }

    // РРјСЏ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ РїРѕ session_id
    #[no_mangle]
    pub extern "C" fn get_session_user_info(path: *mut u16, n: u32, _session_id: u32) -> u32 {
        unsafe {
            if !path.is_null() && n > 0 {
                *path = 0;
            }
        }
        0
    }
}

// Р’РђР–РќРћ: СѓРґР°Р»РёС‚Рµ/Р·Р°РєРѕРјРјРµРЅС‚РёСЂСѓР№С‚Рµ Р’Р•Р—Р”Р• СЃС‚СЂРѕРєРё РІРёРґР° `#[link(name = "sas")] extern "system" { ... }`.
// РћР±СЉСЏРІР»РµРЅРёСЏ `extern "C" { fn ...; }` Р±РµР· #[link(...)] РјРѕР¶РµС‚Рµ РѕСЃС‚Р°РІРёС‚СЊ вЂ” РѕРЅРё СЃРІСЏР¶СѓС‚СЃСЏ СЃ РЅР°С€РёРјРё #[no_mangle] СЂРµР°Р»РёР·Р°С†РёСЏРјРё.



