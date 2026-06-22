#[cfg(target_os = "windows")]
use super::log_native_desktop;
#[cfg(target_os = "windows")]
use windows::core::{BOOL, PCWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, LPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, ShowWindow, SW_HIDE, SW_SHOW,
};

/// 设置 Windows Explorer 原生桌面图标 ListView 的可见性，避免影响桌面背景
#[cfg(target_os = "windows")]
pub fn set_native_desktop_icons_hidden(hidden: bool) -> Result<(), String> {
    log_native_desktop("native_icons.set_hidden", format!("begin hidden={hidden}"));
    let desktop_list_view = find_desktop_list_view()
        .ok_or_else(|| "无法定位 Windows 桌面图标窗口，Explorer 可能尚未完成初始化".to_string())?;
    let command = if hidden { SW_HIDE } else { SW_SHOW };

    unsafe {
        log_native_desktop(
            "native_icons.set_hidden",
            format!(
                "before ShowWindow list={} hidden={hidden}",
                hwnd_value(desktop_list_view)
            ),
        );
        let result = ShowWindow(desktop_list_view, command);
        log_native_desktop(
            "native_icons.set_hidden",
            format!(
                "after ShowWindow list={} previous_visible={}",
                hwnd_value(desktop_list_view),
                result.as_bool()
            ),
        );
    }

    log_native_desktop("native_icons.set_hidden", "completed");
    Ok(())
}

/// 非 Windows 平台没有 Explorer 桌面图标层，保持显式错误避免前端误判已生效
#[cfg(not(target_os = "windows"))]
pub fn set_native_desktop_icons_hidden(_hidden: bool) -> Result<(), String> {
    Err("当前平台暂不支持隐藏 Windows 原生桌面图标".to_string())
}

#[cfg(target_os = "windows")]
fn find_desktop_list_view() -> Option<HWND> {
    log_native_desktop("native_icons.find_desktop_list_view", "begin");
    let def_view = find_shell_def_view()?;
    let list_view_class = to_wide_null("SysListView32");

    let result = unsafe {
        FindWindowExW(
            Some(def_view),
            None,
            PCWSTR(list_view_class.as_ptr()),
            PCWSTR::null(),
        )
        .ok()
    };
    log_native_desktop(
        "native_icons.find_desktop_list_view",
        format!(
            "def_view={} result={}",
            hwnd_value(def_view),
            result
                .map(hwnd_value)
                .map(|value| format!("0x{value:x}"))
                .unwrap_or_else(|| "none".to_string())
        ),
    );
    result
}

#[cfg(target_os = "windows")]
fn find_shell_def_view() -> Option<HWND> {
    log_native_desktop("native_icons.find_shell_def_view", "begin");
    let progman_class = to_wide_null("Progman");
    let def_view_class = to_wide_null("SHELLDLL_DefView");

    let progman =
        unsafe { FindWindowExW(None, None, PCWSTR(progman_class.as_ptr()), PCWSTR::null()).ok() };
    if let Some(progman) = progman {
        log_native_desktop(
            "native_icons.find_shell_def_view",
            format!("Progman={}", hwnd_value(progman)),
        );
        if let Ok(def_view) = unsafe {
            FindWindowExW(
                Some(progman),
                None,
                PCWSTR(def_view_class.as_ptr()),
                PCWSTR::null(),
            )
        } {
            log_native_desktop(
                "native_icons.find_shell_def_view",
                format!("found under Progman def_view={}", hwnd_value(def_view)),
            );
            return Some(def_view);
        }
    }

    let mut search = ShellDefViewSearch { hwnd: None };
    unsafe {
        log_native_desktop("native_icons.find_shell_def_view", "before EnumWindows");
        let _ = EnumWindows(
            Some(enum_windows_proc),
            LPARAM((&mut search as *mut ShellDefViewSearch) as isize),
        );
    }

    log_native_desktop(
        "native_icons.find_shell_def_view",
        format!(
            "EnumWindows result={}",
            search
                .hwnd
                .map(hwnd_value)
                .map(|value| format!("0x{value:x}"))
                .unwrap_or_else(|| "none".to_string())
        ),
    );
    search.hwnd
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam.0 as *mut ShellDefViewSearch);
    let class_name = to_wide_null("SHELLDLL_DefView");

    if let Ok(def_view) = FindWindowExW(
        Some(hwnd),
        None,
        PCWSTR(class_name.as_ptr()),
        PCWSTR::null(),
    ) {
        log_native_desktop(
            "native_icons.enum_windows_proc",
            format!(
                "found SHELLDLL_DefView parent={} def_view={}",
                hwnd_value(hwnd),
                hwnd_value(def_view)
            ),
        );
        search.hwnd = Some(def_view);
        return BOOL(0);
    }

    BOOL(1)
}

#[cfg(target_os = "windows")]
struct ShellDefViewSearch {
    hwnd: Option<HWND>,
}

#[cfg(target_os = "windows")]
fn to_wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(target_os = "windows")]
fn hwnd_value(hwnd: HWND) -> usize {
    hwnd.0 as usize
}
