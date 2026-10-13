//! Windows Shell 图像解析流程，负责按缩略图、快捷方式和 PIDL 图标顺序回退。

use std::io;
use std::mem::size_of;
use std::path::Path;

use super::{app_user_model_id, bitmap};
use crate::domain::filesystem::WINDOWS_SHORTCUT_EXTENSION;
use crate::infrastructure::windows::common::{com, error, wide};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{MAX_PATH, SIZE};
use windows::Win32::Graphics::Gdi::DeleteObject;
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    STGM_READ,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, IShellLinkW, SHCreateItemFromParsingName, SHGetFileInfoW,
    SHParseDisplayName, ShellLink, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_PIDL, SIIGBF,
    SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, SIIGBF_SCALEUP,
};
use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;

/// Shell 图像请求尺寸，兼顾桌面图标清晰度和 base64 传输体积。
const SHELL_IMAGE_SIZE: i32 = 96;
/// Store/AppX 应用在 Shell 命名空间中的解析名前缀。
const APPS_FOLDER_PARSING_NAME_PREFIX: &str = "shell:AppsFolder\\";
/// Shell 虚拟对象解析名常用 `::{GUID}` 形式，需走 PIDL 分支。
const SHELL_NAMESPACE_PREFIX: &str = "::";
/// 浏览器可直接渲染的 PNG data URL 前缀。
const PNG_DATA_URL_PREFIX: &str = "data:image/png;base64,";

/// 按 Windows Shell 默认逻辑提取缩略图或图标，并编码成浏览器可直接渲染的 PNG data URL。
pub fn resolve_shell_image_data_url(path: &Path) -> io::Result<Option<String>> {
    if is_shortcut_path(path) {
        if let Ok(Some(icon)) = resolve_shortcut_icon_data_url(path) {
            return Ok(Some(icon));
        }
    }

    resolve_shell_image_data_url_for_parsing_name(&path.to_string_lossy())
}

/// Shell 虚拟项与真实路径共用图像工厂，保证系统图标、缩略图和透明边缘一致。
pub fn resolve_shell_image_data_url_for_parsing_name(
    parsing_name: &str,
) -> io::Result<Option<String>> {
    if let Ok(Some(thumbnail)) = resolve_shell_thumbnail_data_url(parsing_name) {
        return Ok(Some(thumbnail));
    }

    resolve_shell_icon_data_url(parsing_name)
}

/// 读取 `.lnk` 内部目标；Store/AppX 快捷方式没有普通文件目标，需要从 AUMID 或 PIDL 解析真实图标。
fn resolve_shortcut_icon_data_url(path: &Path) -> io::Result<Option<String>> {
    // Store 快捷方式命中 AUMID 后不需要加载 ShellLink，先走轻量二进制扫描可以减少普通扫描时的 COM 成本。
    if let Some(app_user_model_id) = app_user_model_id::extract_from_shortcut_file(path)? {
        if let Ok(Some(icon)) = resolve_apps_folder_icon_data_url(&app_user_model_id) {
            return Ok(Some(icon));
        }
    }

    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let shortcut_path = wide::path_null_terminated(path);

    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
            .map_err(error::io_other)?;
    let persist_file: IPersistFile = shell_link.cast().map_err(error::io_other)?;
    unsafe {
        persist_file
            .Load(PCWSTR(shortcut_path.as_ptr()), STGM_READ)
            .map_err(error::io_other)?;
    }

    // 普通文件快捷方式仍交给路径 Shell 图标逻辑，保留自定义 icon location 和现有表现。
    if shortcut_has_file_target(&shell_link) {
        return Ok(None);
    }

    let pidl = unsafe { shell_link.GetIDList().map_err(error::io_other)? };
    if pidl.is_null() {
        return Ok(None);
    }

    let result = unsafe { resolve_pidl_icon_data_url(pidl) };
    unsafe {
        com::free_cotaskmem_ptr(pidl);
    }

    result
}

/// AppsFolder 应用入口使用纯图标模式，避免 Shell 返回带磁贴背景或大内边距的缩略图。
fn resolve_apps_folder_icon_data_url(app_user_model_id: &str) -> io::Result<Option<String>> {
    let parsing_name = format!("{APPS_FOLDER_PARSING_NAME_PREFIX}{app_user_model_id}");
    if let Ok(Some(icon)) = resolve_shell_icon_only_image_data_url(&parsing_name) {
        return Ok(Some(icon));
    }

    resolve_shell_image_data_url_for_parsing_name(&parsing_name)
}

/// `IShellLink::GetPath` 为空时通常代表目标是 Applications 等 Shell 命名空间对象。
fn shortcut_has_file_target(shell_link: &IShellLinkW) -> bool {
    let mut target_path = vec![0_u16; MAX_PATH as usize];
    if unsafe {
        shell_link
            .GetPath(&mut target_path, std::ptr::null_mut(), 0)
            .is_err()
    } {
        return false;
    }

    target_path.first().copied().unwrap_or_default() != 0
}

/// Explorer 同源的 Shell 图像工厂会优先返回文件缩略图，普通文件则返回系统图标。
fn resolve_shell_thumbnail_data_url(parsing_name: &str) -> io::Result<Option<String>> {
    resolve_shell_item_image_data_url(parsing_name, SIIGBF_BIGGERSIZEOK)
}

/// Store 应用图标使用 `ICONONLY | SCALEUP`，绕开缩略图/磁贴语义并让小尺寸图标由 Shell 放大。
fn resolve_shell_icon_only_image_data_url(parsing_name: &str) -> io::Result<Option<String>> {
    resolve_shell_item_image_data_url(parsing_name, SIIGBF_ICONONLY | SIIGBF_SCALEUP)
}

/// 通过 `IShellItemImageFactory` 统一读取缩略图或纯图标，差异只由调用方传入的 Shell flags 决定。
fn resolve_shell_item_image_data_url(
    parsing_name: &str,
    flags: SIIGBF,
) -> io::Result<Option<String>> {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let wide_path = wide::null_terminated(parsing_name);
    let image_factory: IShellItemImageFactory = unsafe {
        SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None).map_err(error::io_other)?
    };
    let bitmap = unsafe {
        image_factory
            .GetImage(
                SIZE {
                    cx: SHELL_IMAGE_SIZE,
                    cy: SHELL_IMAGE_SIZE,
                },
                flags,
            )
            .map_err(error::io_other)?
    };

    let png = unsafe { bitmap::bitmap_to_png(bitmap) }?;
    unsafe {
        let _ = DeleteObject(bitmap.into());
    }

    Ok(Some(png_data_url(&png)))
}

/// Shell 图像工厂不可用时退回路径对应的大图标，保证每个文件项都有可识别展示。
fn resolve_shell_icon_data_url(parsing_name: &str) -> io::Result<Option<String>> {
    if parsing_name.starts_with(SHELL_NAMESPACE_PREFIX) {
        return resolve_shell_icon_data_url_from_pidl(parsing_name);
    }

    let wide_path = wide::null_terminated(parsing_name);
    let mut file_info = SHFILEINFOW::default();
    let flags = SHGFI_ICON | SHGFI_LARGEICON;

    let result = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide_path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut file_info),
            size_of::<SHFILEINFOW>() as u32,
            flags,
        )
    };

    if result == 0 || file_info.hIcon.is_invalid() {
        return Ok(None);
    }

    let pixels = unsafe { bitmap::icon_to_png_rgba(file_info.hIcon) };
    unsafe {
        let _ = DestroyIcon(file_info.hIcon);
    }

    pixels.map(|png| Some(png_data_url(&png)))
}

/// `::{GUID}` 这类虚拟项需先转 PIDL 再取图标，否则部分系统不会给出 HICON。
fn resolve_shell_icon_data_url_from_pidl(parsing_name: &str) -> io::Result<Option<String>> {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let wide_path = wide::null_terminated(parsing_name);
    let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
    unsafe {
        SHParseDisplayName(PCWSTR(wide_path.as_ptr()), None, &mut pidl, 0, None)
            .map_err(error::io_other)?;
    }

    let result = unsafe { resolve_pidl_icon_data_url(pidl) };
    unsafe {
        com::free_cotaskmem_ptr(pidl);
    }

    result
}

/// 通过 PIDL 调用 `SHGetFileInfoW`，覆盖无文件路径的 Shell 命名空间对象。
unsafe fn resolve_pidl_icon_data_url(pidl: *mut ITEMIDLIST) -> io::Result<Option<String>> {
    let mut file_info = SHFILEINFOW::default();
    let result = SHGetFileInfoW(
        PCWSTR(pidl as *const u16),
        FILE_FLAGS_AND_ATTRIBUTES(0),
        Some(&mut file_info),
        size_of::<SHFILEINFOW>() as u32,
        SHGFI_PIDL | SHGFI_ICON | SHGFI_LARGEICON,
    );

    if result == 0 || file_info.hIcon.is_invalid() {
        return Ok(None);
    }

    let pixels = bitmap::icon_to_png_rgba(file_info.hIcon);
    let _ = DestroyIcon(file_info.hIcon);

    pixels.map(|png| Some(png_data_url(&png)))
}

/// 只对 `.lnk` 启用 ShellLink 解析：ShellLink 读不懂 `.url` 的 INI 文本格式，
/// Internet 快捷方式交给通用 Shell 路径解析即可拿到与 Explorer 一致的图标。
fn is_shortcut_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(WINDOWS_SHORTCUT_EXTENSION))
}

/// 将 PNG 字节包装成浏览器 `<img>` 可以直接消费的 data URL。
fn png_data_url(png: &[u8]) -> String {
    format!("{PNG_DATA_URL_PREFIX}{}", STANDARD.encode(png))
}
