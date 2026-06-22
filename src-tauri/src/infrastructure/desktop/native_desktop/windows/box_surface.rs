//! Box 和桌面背景层窗口实现，负责 Win32 窗口类、布局、绘制和命中测试。

use super::*;

unsafe extern "system" fn layer_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            log_native_desktop(
                "layer_window_proc",
                format!("WM_CREATE hwnd={}", hwnd_value(hwnd)),
            );
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_SIZE => {
            if !GLOBAL_CONTROLLER.is_null() {
                (&mut *GLOBAL_CONTROLLER).resize_layer(hwnd);
            }
            LRESULT(0)
        }
        WM_NCHITTEST => {
            // 背景层不参与鼠标命中，确保 Explorer 桌面继续接收右键、框选和拖拽。
            LRESULT(windows::Win32::UI::WindowsAndMessaging::HTTRANSPARENT as isize)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn box_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            log_native_desktop(
                "box_window_proc",
                format!("WM_CREATE hwnd={}", hwnd_value(hwnd)),
            );
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_PAINT => {
            paint_box_background(hwnd);
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn box_drag_handle_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            log_native_desktop(
                "box_drag_handle_window_proc",
                format!("WM_CREATE hwnd={}", hwnd_value(hwnd)),
            );
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_NCHITTEST => {
            if resolve_box_drag_handle_enabled(hwnd) {
                LRESULT(HTCAPTION as isize)
            } else {
                LRESULT(HTTRANSPARENT as isize)
            }
        }
        WM_EXITSIZEMOVE => {
            if !GLOBAL_CONTROLLER.is_null() {
                (&mut *GLOBAL_CONTROLLER).handle_box_move_finished(hwnd);
            }
            LRESULT(0)
        }
        WM_WINDOWPOSCHANGED => {
            if !GLOBAL_CONTROLLER.is_null() {
                (&mut *GLOBAL_CONTROLLER).sync_panel_position_from_drag_handle(hwnd);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

/// 拖动句柄只在 Box 未锁定时参与命中，锁定后把标题栏也交还给 Explorer。
fn resolve_box_drag_handle_enabled(hwnd: HWND) -> bool {
    let Some(box_state) = (unsafe {
        if GLOBAL_CONTROLLER.is_null() {
            None
        } else {
            (&mut *GLOBAL_CONTROLLER).box_state_by_hwnd(hwnd)
        }
    }) else {
        return false;
    };
    !box_state.locked
}

pub(super) unsafe fn register_layer_class() -> Result<(), String> {
    register_window_class(DESKTOP_LAYER_CLASS, layer_window_proc)
}

pub(super) unsafe fn register_box_class() -> Result<(), String> {
    register_window_class(NATIVE_BOX_CLASS, box_window_proc)?;
    register_window_class(NATIVE_BOX_DRAG_HANDLE_CLASS, box_drag_handle_window_proc)
}

unsafe fn register_window_class(
    class_name: &str,
    window_proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
) -> Result<(), String> {
    let instance = GetModuleHandleW(None).map_err(|error| format!("无法获取模块句柄：{error}"))?;
    let class_name_wide = to_wide(class_name);
    let cursor =
        LoadCursorW(None, IDC_ARROW).map_err(|error| format!("无法加载鼠标指针：{error}"))?;
    let window_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        hCursor: cursor,
        lpszClassName: PCWSTR(class_name_wide.as_ptr()),
        ..WNDCLASSW::default()
    };

    SetLastError(WIN32_ERROR(0));
    let atom = RegisterClassW(&window_class);
    if atom != 0 {
        return Ok(());
    }
    let error = GetLastError();
    if error == ERROR_CLASS_ALREADY_EXISTS {
        return Ok(());
    }
    Err(format!("无法注册窗口类 {class_name}：错误码 {}", error.0))
}

pub(super) unsafe fn create_layer_window() -> Result<HWND, String> {
    let instance =
        GetModuleHandleW(None).map_err(|error| format!("无法获取原生桌面模块句柄：{error}"))?;
    let class_name = to_wide(DESKTOP_LAYER_CLASS);
    let title = to_wide("Dasktop Explorer Native Layer");
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
        PCWSTR(class_name.as_ptr()),
        PCWSTR(title.as_ptr()),
        WS_POPUP | WS_VISIBLE | WS_DISABLED,
        0,
        0,
        1,
        1,
        None,
        None,
        Some(instance.into()),
        None,
    )
    .map_err(|error| format!("无法创建原生桌面背景层：{error}"))?;
    Ok(hwnd)
}

pub(super) unsafe fn create_box_windows(
    box_state: &NativeBoxState,
    display: &NativeDesktopDisplaySettings,
) -> Result<NativeBoxWindows, String> {
    let instance =
        GetModuleHandleW(None).map_err(|error| format!("无法获取 Box 模块句柄：{error}"))?;
    let title = to_wide(&box_state.title);
    let (x, y, width, height) = resolve_box_screen_rect(box_state);
    // Box 采用独立顶层 layered overlay，不再进入 Explorer 子窗口树，避免“盖住图标”和“被 ListView 盖住”二选一。
    // 视觉面板保持 WS_EX_TRANSPARENT，让 Explorer 的 OLE 拖放命中不会落到 Dasktop 面板上。
    let panel_class_name = to_wide(NATIVE_BOX_CLASS);
    let panel_ex_style = WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED;
    let style = WS_POPUP | WS_VISIBLE;
    log_native_desktop(
        "win32.create_box_windows",
        format!(
            "before id={} title={} rect=({}, {}) {}x{} style=0x{:x} panel_ex=0x{:x}",
            box_state.id, box_state.title, x, y, width, height, style.0, panel_ex_style.0
        ),
    );
    // CreateWindowExW 失败时 windows-rs 会读取线程最后错误码；先清零可避免 RegisterClassW
    // “类已存在”等历史错误污染诊断日志，便于后续精确判断桌面层级或样式问题。
    SetLastError(WIN32_ERROR(0));
    let panel = CreateWindowExW(
        panel_ex_style,
        PCWSTR(panel_class_name.as_ptr()),
        PCWSTR(title.as_ptr()),
        style,
        x,
        y,
        width,
        height,
        None,
        None,
        Some(instance.into()),
        None,
    )
    .map_err(|error| {
        let last_error = GetLastError();
        format!(
            "无法创建原生 Box 背景：{error}，last_error=0x{:08x}",
            last_error.0
        )
    })?;
    apply_box_window_alpha(panel, display.box_background_opacity);

    let (handle_x, handle_y, handle_width, handle_height) = resolve_box_drag_handle_rect(box_state);
    let handle_class_name = to_wide(NATIVE_BOX_DRAG_HANDLE_CLASS);
    let drag_handle = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
        PCWSTR(handle_class_name.as_ptr()),
        PCWSTR(title.as_ptr()),
        style,
        handle_x,
        handle_y,
        handle_width,
        handle_height,
        None,
        None,
        Some(instance.into()),
        None,
    )
    .map_err(|error| {
        let last_error = GetLastError();
        format!(
            "无法创建原生 Box 拖动句柄：{error}，last_error=0x{:08x}",
            last_error.0
        )
    })?;
    apply_box_window_alpha(drag_handle, 1);
    log_window_snapshot("win32.create_box_windows.panel.after", panel);
    log_window_snapshot("win32.create_box_windows.handle.after", drag_handle);
    Ok(NativeBoxWindows { drag_handle, panel })
}

pub(super) fn layout_box_windows(
    windows: NativeBoxWindows,
    box_state: &NativeBoxState,
    display: &NativeDesktopDisplaySettings,
) {
    let (x, y, width, height) = resolve_box_screen_rect(box_state);
    apply_box_window_alpha(windows.panel, display.box_background_opacity);
    unsafe {
        let _ = SetWindowPos(
            windows.panel,
            Some(HWND_TOP),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let (handle_x, handle_y, handle_width, handle_height) =
            resolve_box_drag_handle_rect(box_state);
        let _ = SetWindowPos(
            windows.drag_handle,
            Some(HWND_TOP),
            handle_x,
            handle_y,
            handle_width,
            handle_height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    log_native_desktop(
        "win32.layout_box_windows",
        format!(
            "id={} rect=({}, {}) {}x{} opacity={} panel={} handle={}",
            box_state.id,
            x,
            y,
            width,
            height,
            display.box_background_opacity,
            describe_window(windows.panel),
            describe_window(windows.drag_handle)
        ),
    );
    log_window_snapshot("win32.layout_box_windows.panel.after", windows.panel);
    log_window_snapshot("win32.layout_box_windows.handle.after", windows.drag_handle);
}

pub(super) fn resolve_box_screen_rect(box_state: &NativeBoxState) -> (i32, i32, i32, i32) {
    let height = if box_state.collapsed {
        BOX_TITLE_HEIGHT
    } else {
        box_state.height
    };

    (box_state.x, box_state.y, box_state.width, height)
}

/// 标题拖动句柄只覆盖标题栏，避免 Box 内容区顶层窗口挡住 Explorer 的图标拖放判定。
fn resolve_box_drag_handle_rect(box_state: &NativeBoxState) -> (i32, i32, i32, i32) {
    let (_, y, width, height) = resolve_box_screen_rect(box_state);
    let handle_y = if box_state.title_position == NativeBoxTitlePosition::Bottom {
        y + height - BOX_TITLE_HEIGHT
    } else {
        y
    };

    (box_state.x, handle_y, width, BOX_TITLE_HEIGHT)
}

fn apply_box_window_alpha(hwnd: HWND, opacity_percent: i32) {
    let alpha = ((opacity_percent.clamp(10, 100) * 255) / 100) as u8;
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
    }
}

fn paint_box_background(hwnd: HWND) {
    let render_model = unsafe {
        if GLOBAL_CONTROLLER.is_null() {
            None
        } else {
            (&mut *GLOBAL_CONTROLLER).box_render_model_by_hwnd(hwnd)
        }
    };
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        if hdc.is_invalid() {
            return;
        }
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_ok() {
            let box_state = render_model.as_ref().map(|model| &model.box_state);
            let display = render_model.as_ref().map(|model| &model.display);
            let is_dark = box_state
                .as_ref()
                .map(|value| value.theme.eq_ignore_ascii_case("dark"))
                .unwrap_or(false);
            let radius = display
                .as_ref()
                .map(|value| value.corner_radius.clamp(0, 24))
                .unwrap_or(8);
            let opacity = display
                .as_ref()
                .map(|value| value.box_background_opacity)
                .unwrap_or(70);
            let colors = resolve_box_panel_colors(is_dark, opacity);
            let text = if is_dark {
                rgb_color(247, 248, 251)
            } else {
                rgb_color(23, 24, 28)
            };
            let brush = CreateSolidBrush(colors.background);
            let pen = CreatePen(PS_SOLID, 1, colors.border);
            let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
            let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
            let _ = windows::Win32::Graphics::Gdi::RoundRect(
                hdc,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                radius,
                radius,
            );
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);
            let _ = DeleteObject(brush.into());
            let _ = DeleteObject(pen.into());

            let title_position = box_state
                .as_ref()
                .map(|value| value.title_position.clone())
                .unwrap_or(NativeBoxTitlePosition::Top);
            let header_rect = if title_position == NativeBoxTitlePosition::Bottom {
                RECT {
                    left: rect.left,
                    top: rect.bottom - BOX_TITLE_HEIGHT,
                    right: rect.right,
                    bottom: rect.bottom,
                }
            } else {
                RECT {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.top + BOX_TITLE_HEIGHT,
                }
            };
            let header_brush = CreateSolidBrush(colors.header);
            let _ = FillRect(hdc, &header_rect, header_brush);
            let _ = DeleteObject(header_brush.into());

            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, text);
            let old_font = SelectObject(hdc, GetStockObject(DEFAULT_GUI_FONT));
            let mut text_rect = RECT {
                left: header_rect.left + 14,
                top: header_rect.top,
                right: header_rect.right - 14,
                bottom: header_rect.bottom,
            };
            let mut title = to_wide(
                box_state
                    .as_ref()
                    .map(|value| value.title.as_str())
                    .unwrap_or("Box"),
            );
            let _ = DrawTextW(
                hdc,
                &mut title,
                &mut text_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            if let Some(model) = render_model.as_ref() {
                paint_box_items(hdc, &rect, model, text);
            }
            let _ = SelectObject(hdc, old_font);
        }
        let _ = EndPaint(hwnd, &paint);
    }
}

fn paint_box_items(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    rect: &RECT,
    model: &NativeBoxRenderModel,
    text_color: COLORREF,
) {
    if model.box_state.collapsed {
        return;
    }

    unsafe {
        let _ = SetStretchBltMode(hdc, HALFTONE);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, text_color);
    }

    for item in &model.items {
        let item_rect = resolve_box_item_rect(rect, model, item.order_index);
        draw_box_item_tile(hdc, &item_rect, &model.display, item, text_color);
    }
}

fn draw_box_item_tile(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    item_rect: &RECT,
    display: &NativeDesktopDisplaySettings,
    item: &NativeBoxRenderItem,
    text_color: COLORREF,
) {
    let icon_size = display.icon_size.clamp(28, 96);
    let icon_left = item_rect.left + ((item_rect.right - item_rect.left - icon_size) / 2).max(0);
    let icon_top = item_rect.top;
    let rendered_icon = item
        .icon_data_url
        .as_deref()
        .and_then(decode_png_data_url)
        .map(|icon| unsafe {
            draw_decoded_icon(hdc, icon_left, icon_top, icon_size, &icon);
        })
        .is_some();
    if !rendered_icon {
        draw_icon_placeholder(hdc, icon_left, icon_top, icon_size, text_color);
    }

    if display.show_item_labels {
        let mut label_rect = RECT {
            left: item_rect.left,
            top: icon_top + icon_size + 6,
            right: item_rect.right,
            bottom: item_rect.bottom,
        };
        let mut label = to_wide(&item.name);
        unsafe {
            SetTextColor(hdc, text_color);
            let _ = DrawTextW(
                hdc,
                &mut label,
                &mut label_rect,
                DT_CENTER | DT_END_ELLIPSIS,
            );
        }
    }
}

fn resolve_box_item_rect(rect: &RECT, model: &NativeBoxRenderModel, order_index: i32) -> RECT {
    let content_top = if model.box_state.title_position == NativeBoxTitlePosition::Top {
        BOX_TITLE_HEIGHT + BOX_CONTENT_PADDING
    } else {
        BOX_CONTENT_PADDING
    };
    let content_bottom = if model.box_state.title_position == NativeBoxTitlePosition::Bottom {
        rect.bottom - BOX_TITLE_HEIGHT - BOX_CONTENT_PADDING
    } else {
        rect.bottom - BOX_CONTENT_PADDING
    };
    let slot_width = (model.display.label_width + model.display.icon_gap_x + 24).max(88);
    let slot_height = (model.display.icon_size
        + model.display.icon_gap_y
        + (model.display.label_text_size as i32 * 2)
        + 28)
        .max(86);
    let content_width = (rect.right - rect.left - BOX_CONTENT_PADDING * 2).max(slot_width);
    let columns = (content_width / slot_width).max(1);
    let index = order_index.max(0);
    let column = index % columns;
    let row = index / columns;
    let left = rect.left + BOX_CONTENT_PADDING + column * slot_width;
    let top = rect.top + content_top + row * slot_height;

    RECT {
        left,
        top,
        right: (left + slot_width).min(rect.right - BOX_CONTENT_PADDING),
        bottom: (top + slot_height).min(content_bottom),
    }
}

fn draw_icon_placeholder(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    left: i32,
    top: i32,
    size: i32,
    text_color: COLORREF,
) {
    unsafe {
        let brush = CreateSolidBrush(blend_rgb((210, 216, 226), (166, 176, 192), 65));
        let pen = CreatePen(PS_SOLID, 1, text_color);
        let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = windows::Win32::Graphics::Gdi::RoundRect(
            hdc,
            left,
            top,
            left + size,
            top + size,
            10,
            10,
        );
        let _ = SelectObject(hdc, old_pen);
        let _ = SelectObject(hdc, old_brush);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}

unsafe fn draw_decoded_icon(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    left: i32,
    top: i32,
    size: i32,
    icon: &DecodedPngIcon,
) {
    let mut bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: icon.width as i32,
            biHeight: -(icon.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let _ = StretchDIBits(
        hdc,
        left,
        top,
        size,
        size,
        0,
        0,
        icon.width as i32,
        icon.height as i32,
        Some(icon.bgra.as_ptr().cast()),
        &mut bitmap_info,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
}

struct DecodedPngIcon {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

fn decode_png_data_url(data_url: &str) -> Option<DecodedPngIcon> {
    let payload = data_url.strip_prefix("data:image/png;base64,")?;
    let bytes = BASE64_STANDARD.decode(payload).ok()?;
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let output = reader.next_frame(&mut buffer).ok()?;
    let rgba = normalize_png_rgba(&buffer[..output.buffer_size()], output.color_type)?;
    let mut bgra = rgba;
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(DecodedPngIcon {
        width: output.width,
        height: output.height,
        bgra,
    })
}

fn normalize_png_rgba(bytes: &[u8], color_type: png::ColorType) -> Option<Vec<u8>> {
    match color_type {
        png::ColorType::Rgba => Some(bytes.to_vec()),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((bytes.len() / 3) * 4);
            for pixel in bytes.chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            Some(rgba)
        }
        png::ColorType::Grayscale => Some(
            bytes
                .iter()
                .flat_map(|value| [*value, *value, *value, 255])
                .collect(),
        ),
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity((bytes.len() / 2) * 4);
            for pixel in bytes.chunks_exact(2) {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            Some(rgba)
        }
        png::ColorType::Indexed => None,
    }
}

struct BoxPanelColors {
    background: COLORREF,
    border: COLORREF,
    header: COLORREF,
}

/// 根据用户透明度偏好生成非 layered 子窗口可用的面板颜色。
///
/// Explorer 子窗口层级不稳定支持真实 alpha，这里用颜色强度模拟旧 WebView 的半透明灰白视觉。
fn resolve_box_panel_colors(is_dark: bool, opacity_percent: i32) -> BoxPanelColors {
    let strength = opacity_percent.clamp(0, 100);
    if is_dark {
        return BoxPanelColors {
            background: blend_rgb((18, 20, 24), (34, 38, 47), strength),
            border: blend_rgb((42, 46, 56), (74, 82, 98), strength),
            header: blend_rgb((24, 27, 34), (44, 49, 60), strength),
        };
    }

    BoxPanelColors {
        background: blend_rgb((248, 250, 252), (226, 232, 243), strength),
        border: blend_rgb((220, 225, 235), (174, 183, 198), strength),
        header: blend_rgb((250, 251, 253), (236, 240, 247), strength),
    }
}

/// 在两个 RGB 端点之间插值，避免 Win32 GDI 绘制的 Box 在浅色主题下变成刺眼纯白。
fn blend_rgb(from: (u8, u8, u8), to: (u8, u8, u8), percent: i32) -> COLORREF {
    let strength = percent.clamp(0, 100);
    let mix = |start: u8, end: u8| -> u8 {
        let start = start as i32;
        let end = end as i32;
        (start + ((end - start) * strength) / 100).clamp(0, 255) as u8
    };

    rgb_color(mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
}

/// COLORREF 使用 0x00bbggrr 排列，集中封装能避免后续手写十六进制时把颜色通道写反。
fn rgb_color(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF((red as u32) | ((green as u32) << 8) | ((blue as u32) << 16))
}
