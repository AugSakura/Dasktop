use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::desktop::{DesktopItem, DesktopItemKind, DesktopItemSource, DesktopSnapshot};

const SHELL_ITEM_PATH_PREFIX: &str = "shell::";

/// 已知 Shell 虚拟桌面项使用稳定业务 ID 映射，避免把不可持久化的 PIDL 直接写入前端数据库
struct KnownShellDesktopItem {
    id: &'static str,
    name: &'static str,
    parsing_name: &'static str,
}

const KNOWN_SHELL_DESKTOP_ITEMS: &[KnownShellDesktopItem] = &[
    KnownShellDesktopItem {
        id: "this-pc",
        name: "此电脑",
        parsing_name: "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}",
    },
    KnownShellDesktopItem {
        id: "user-files",
        name: "用户的文件",
        parsing_name: "::{59031A47-3F72-44A7-89C5-5595FE6B30EE}",
    },
    KnownShellDesktopItem {
        id: "control-panel",
        name: "控制面板",
        parsing_name: "::{5399E694-6CE5-4D6C-8FCE-1D8870FDCBA0}",
    },
    KnownShellDesktopItem {
        id: "network",
        name: "网络",
        parsing_name: "::{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}",
    },
    KnownShellDesktopItem {
        id: "recycle-bin",
        name: "回收站",
        parsing_name: "::{645FF040-5081-101B-9F08-00AA002F954E}",
    },
];

/// 扫描用户桌面目录，返回用于 Box 映射、设置页展示和 Explorer 图标匹配的轻量元信息
pub fn scan_desktop() -> io::Result<DesktopSnapshot> {
    let desktop_path = resolve_desktop_path();
    let mut items = Vec::new();

    if desktop_path.exists() {
        for entry in fs::read_dir(&desktop_path)? {
            let entry = entry?;
            let path = entry.path();

            items.push(create_desktop_item(&path)?);
        }
    }

    items.extend(
        KNOWN_SHELL_DESKTOP_ITEMS
            .iter()
            .map(create_shell_desktop_item),
    );
    items.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));

    Ok(DesktopSnapshot {
        desktop_path: desktop_path.to_string_lossy().to_string(),
        items,
    })
}

/// 按真实路径解析项目元信息，支持 Box 收纳任意磁盘文件时继续复用 Windows Shell 图标逻辑
pub fn scan_paths(paths: &[String]) -> io::Result<Vec<DesktopItem>> {
    let mut seen_paths = HashSet::new();
    let mut items = Vec::new();

    for raw_path in paths {
        let normalized_key = normalize_item_key(raw_path);
        if !seen_paths.insert(normalized_key) {
            continue;
        }

        if let Some(shell_item) = create_shell_desktop_item_from_key(raw_path) {
            items.push(shell_item);
            continue;
        }

        let path = PathBuf::from(raw_path);
        if !path.exists() {
            continue;
        }

        items.push(create_desktop_item(&path)?);
    }

    Ok(items)
}

/// 将文件系统路径转换成前端可展示的桌面项目；真实打开方式仍由系统 Shell 处理
fn create_desktop_item(path: &Path) -> io::Result<DesktopItem> {
    let metadata = fs::metadata(path)?;

    Ok(DesktopItem {
        id: stable_item_id(path),
        name: resolve_item_name(path),
        path: path.to_string_lossy().to_string(),
        extension: path
            .extension()
            .map(|value| value.to_string_lossy().to_string()),
        kind: resolve_item_kind(path, metadata.is_dir()),
        icon_data_url: resolve_item_icon_data_url(path),
        source: DesktopItemSource::FileSystem,
        shell_id: None,
    })
}

/// 根据稳定 `shell::` 键还原虚拟桌面项，供 Box 启动补齐和原生拖放落库复用
fn create_shell_desktop_item_from_key(raw_path: &str) -> Option<DesktopItem> {
    let shell_id = raw_path.strip_prefix(SHELL_ITEM_PATH_PREFIX)?;

    KNOWN_SHELL_DESKTOP_ITEMS
        .iter()
        .find(|item| item.id.eq_ignore_ascii_case(shell_id))
        .map(create_shell_desktop_item)
}

/// 构造 Shell 虚拟项时使用 parsing name 提取图标，打开和右键菜单也通过同一标识回到系统 Shell
fn create_shell_desktop_item(item: &KnownShellDesktopItem) -> DesktopItem {
    DesktopItem {
        id: format!("shell_{}", item.id),
        name: item.name.to_string(),
        path: shell_item_path(item),
        extension: None,
        kind: DesktopItemKind::Shell,
        icon_data_url: resolve_item_icon_data_url(Path::new(item.parsing_name)),
        source: DesktopItemSource::Shell,
        shell_id: Some(item.id.to_string()),
    }
}

/// 打开、右键菜单和原生拖放都需要从稳定 ID 回到 Shell parsing name，未知项保持显式不支持
pub fn resolve_shell_parsing_name(shell_id: &str) -> Option<&'static str> {
    KNOWN_SHELL_DESKTOP_ITEMS
        .iter()
        .find(|item| item.id.eq_ignore_ascii_case(shell_id))
        .map(|item| item.parsing_name)
}

/// 根目录这类路径没有 file_name，使用完整路径作为展示名可以避免空白项目
fn resolve_item_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// 优先使用 Windows 用户桌面路径，拿不到用户目录时才退回当前目录保证命令可返回
fn resolve_desktop_path() -> PathBuf {
    if let Ok(profile_path) = env::var("USERPROFILE") {
        return PathBuf::from(profile_path).join("Desktop");
    }

    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// 文件类型只用于选择前端图标，不参与真实文件的打开方式判断
fn resolve_item_kind(path: &Path, is_dir: bool) -> DesktopItemKind {
    if is_dir {
        return DesktopItemKind::Folder;
    }

    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("lnk") => DesktopItemKind::Shortcut,
        Some(_) => DesktopItemKind::File,
        None => DesktopItemKind::Unknown,
    }
}

/// 基于完整路径生成稳定 ID，避免重名文件在前端列表中互相覆盖
fn stable_item_id(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '_'
            }
        })
        .collect()
}

/// Windows 文件系统路径大小写不敏感，Shell 键同样归一化，去重不改变返回给前端的原始主键
fn normalize_item_key(raw_path: &str) -> String {
    if raw_path.starts_with(SHELL_ITEM_PATH_PREFIX) {
        return raw_path.to_lowercase();
    }

    Path::new(raw_path)
        .to_string_lossy()
        .replace('/', "\\")
        .to_lowercase()
}

/// 生成持久化主键时集中处理前缀，避免不同调用点拼出不一致的 Shell 路径
fn shell_item_path(item: &KnownShellDesktopItem) -> String {
    format!("{SHELL_ITEM_PATH_PREFIX}{}", item.id)
}

/// 读取系统 Shell 对该路径解析出的默认展示图像，失败时返回空值交给前端占位图标兜底
#[cfg(target_os = "windows")]
fn resolve_item_icon_data_url(path: &Path) -> Option<String> {
    windows_icon::resolve_shell_image_data_url(path)
        .ok()
        .flatten()
}

/// 非 Windows 平台暂不伪造图标，避免跨平台扫描结果和系统真实图标语义不一致
#[cfg(not(target_os = "windows"))]
fn resolve_item_icon_data_url(_path: &Path) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
mod windows_icon {
    use std::io;
    use std::mem::size_of;
    use std::path::Path;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use png::{BitDepth, ColorType, Encoder};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{GetLastError, SIZE};
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SHGetFileInfoW, SHFILEINFOW,
        SHGFI_ICON, SHGFI_LARGEICON, SIIGBF_BIGGERSIZEOK,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    const SHELL_IMAGE_SIZE: i32 = 96;

    /// 按 Windows Shell 默认逻辑提取缩略图或图标，并编码成浏览器可直接渲染的 PNG data URL
    pub fn resolve_shell_image_data_url(path: &Path) -> io::Result<Option<String>> {
        if let Ok(Some(thumbnail)) = resolve_shell_thumbnail_data_url(path) {
            return Ok(Some(thumbnail));
        }

        resolve_shell_icon_data_url(path)
    }

    /// Explorer 同源的 Shell 图像工厂会优先返回文件缩略图，普通文件则返回系统图标
    fn resolve_shell_thumbnail_data_url(path: &Path) -> io::Result<Option<String>> {
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let wide_path = to_wide_path(path);
        let image_factory: IShellItemImageFactory = unsafe {
            SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None)
                .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?
        };
        let bitmap = unsafe {
            image_factory
                .GetImage(
                    SIZE {
                        cx: SHELL_IMAGE_SIZE,
                        cy: SHELL_IMAGE_SIZE,
                    },
                    SIIGBF_BIGGERSIZEOK,
                )
                .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?
        };

        let png = unsafe { bitmap_to_png(bitmap) }?;
        unsafe {
            let _ = DeleteObject(bitmap.into());
        }

        Ok(Some(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png)
        )))
    }

    /// Shell 图像工厂不可用时，退回路径对应的大图标，保证每个桌面项都有可识别展示
    fn resolve_shell_icon_data_url(path: &Path) -> io::Result<Option<String>> {
        let wide_path = to_wide_path(path);
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

        let pixels = unsafe { icon_to_png_rgba(file_info.hIcon) };
        unsafe {
            let _ = DestroyIcon(file_info.hIcon);
        }

        pixels.map(|png| Some(format!("data:image/png;base64,{}", STANDARD.encode(png))))
    }

    /// Windows API 接收 UTF-16 零结尾路径，保留原始路径可让 `.lnk` 走系统快捷方式解析
    fn to_wide_path(path: &Path) -> Vec<u16> {
        path.to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }

    /// 将 HICON 转换为 PNG 字节；这里显式释放 GDI 对象，避免频繁刷新桌面时泄漏句柄
    unsafe fn icon_to_png_rgba(
        icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    ) -> io::Result<Vec<u8>> {
        let mut icon_info = ICONINFO::default();
        GetIconInfo(icon, &mut icon_info)
            .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;

        let result = bitmap_to_png(icon_info.hbmColor);
        let _ = DeleteObject(icon_info.hbmColor.into());
        let _ = DeleteObject(icon_info.hbmMask.into());

        result
    }

    /// 从彩色位图读取 32 位像素并转为 PNG，保持 Alpha 通道以匹配 Windows 原生图标透明边缘
    unsafe fn bitmap_to_png(bitmap: HBITMAP) -> io::Result<Vec<u8>> {
        if bitmap.is_invalid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "icon color bitmap is empty",
            ));
        }

        let mut bitmap_meta = BITMAP::default();
        let meta_size = GetObjectW(
            bitmap.into(),
            size_of::<BITMAP>() as i32,
            Some(&mut bitmap_meta as *mut _ as *mut _),
        );
        if meta_size == 0 {
            return Err(last_os_error("GetObjectW"));
        }

        let width = bitmap_meta.bmWidth;
        let height = bitmap_meta.bmHeight;
        if width <= 0 || height <= 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "icon bitmap size is invalid",
            ));
        }

        let mut bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bgra = vec![0_u8; (width * height * 4) as usize];
        let device_context = GetDC(None);
        if device_context.is_invalid() {
            return Err(last_os_error("GetDC"));
        }

        let lines = GetDIBits(
            device_context,
            bitmap,
            0,
            height as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        );
        let _ = ReleaseDC(None, device_context);
        if lines == 0 {
            return Err(last_os_error("GetDIBits"));
        }

        let mut rgba = bgra;
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }

        encode_png(width as u32, height as u32, &rgba)
    }

    /// PNG 编码失败不会中断桌面扫描，上层会回退到语义图标
    fn encode_png(width: u32, height: u32, rgba: &[u8]) -> io::Result<Vec<u8>> {
        let mut png_bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut png_bytes, width, height);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
            writer
                .write_image_data(rgba)
                .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
        }

        Ok(png_bytes)
    }

    /// Windows API 失败时携带调用点，便于后续定位具体系统能力问题
    fn last_os_error(operation: &str) -> io::Error {
        let code = unsafe { GetLastError() };
        io::Error::new(
            io::ErrorKind::Other,
            format!("{operation} failed with Windows error {}", code.0),
        )
    }
}
