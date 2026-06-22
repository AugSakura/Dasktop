//! Windows 原生桌面接管实现，封装 Win32 message loop、Explorer ListView 桥接和 Box 绘制。

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::mem::size_of;
use std::ptr;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use tauri::{AppHandle, Emitter};
use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, SetLastError, COLORREF, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, RECT,
    WIN32_ERROR, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    GetStockObject, InvalidateRect, SelectObject, SetBkMode, SetStretchBltMode, SetTextColor,
    StretchDIBits, UpdateWindow, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DEFAULT_GUI_FONT,
    DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, HALFTONE, HGDIOBJ,
    PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, FindWindowExW,
    FindWindowW, GetClassNameW, GetClientRect, GetParent, GetSystemMetrics, GetWindow,
    GetWindowLongW, GetWindowRect, IsWindow, IsWindowVisible, LoadCursorW, PeekMessageW,
    RegisterClassW, SendMessageTimeoutW, SendMessageW, SetLayeredWindowAttributes, SetParent,
    SetWindowLongW, SetWindowPos, ShowWindow, TranslateMessage, CS_DBLCLKS, CS_HREDRAW, CS_VREDRAW,
    GWL_EXSTYLE, GWL_STYLE, GW_CHILD, GW_HWNDNEXT, GW_HWNDPREV, HTCAPTION, HTTRANSPARENT, HWND_TOP,
    IDC_ARROW, LWA_ALPHA, MSG, PM_REMOVE, SMTO_NORMAL, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_SHOW, WM_CREATE,
    WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_NCHITTEST, WM_PAINT, WM_QUIT, WM_SIZE, WM_WINDOWPOSCHANGED,
    WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_DISABLED, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

use super::events::{
    NativeDesktopBoxMoved, NativeDesktopError, NATIVE_DESKTOP_BOX_MOVED_EVENT,
    NATIVE_DESKTOP_ERROR_EVENT,
};
use super::model::{normalize_item_key, NativeDesktopModel};
use super::runtime::{describe_native_desktop_command, NativeDesktopCommand};
use crate::domain::desktop::{
    NativeBoxState, NativeBoxTitlePosition, NativeDesktopDisplaySettings, NativeDesktopState,
};
use crate::infrastructure::desktop::{log_native_desktop, set_native_desktop_icons_hidden};

mod box_surface;
mod display_name;
mod explorer_icon_bridge;

use box_surface::{
    create_box_windows, create_layer_window, layout_box_windows, register_box_class,
    register_layer_class, resolve_box_screen_rect,
};
use explorer_icon_bridge::ExplorerIconBridge;

const DESKTOP_LAYER_CLASS: &str = "DasktopExplorerNativeLayer";
const NATIVE_BOX_CLASS: &str = "DasktopExplorerNativeBoxPanel";
const NATIVE_BOX_DRAG_HANDLE_CLASS: &str = "DasktopExplorerNativeBoxDragHandle";
const DESKTOP_SPAWN_WORKERW_MESSAGE: u32 = 0x052c;
const MESSAGE_IDLE_TIMEOUT: Duration = Duration::from_millis(16);
const EXPLORER_SYNC_INTERVAL: Duration = Duration::from_millis(220);
const BOX_TITLE_HEIGHT: i32 = 40;
const BOX_CONTENT_PADDING: i32 = 16;
const EXPLORER_PARKING_MARGIN: i32 = 10_000;
const REMOTE_TEXT_CHARS: usize = 520;

/// Win32 线程状态只在桌面接管线程访问，通过窗口过程的全局指针间接取得。
pub struct WindowsNativeDesktopController {
    app: AppHandle,
    hwnd: Option<HWND>,
    box_windows: HashMap<String, NativeBoxWindows>,
    explorer_icons: ExplorerIconBridge,
    last_explorer_sync: Instant,
    model: NativeDesktopModel,
}

/// 每个 Box 拆成视觉面板和标题拖动句柄，避免整块面板成为 Explorer 拖放的不可投放目标。
#[derive(Clone, Copy)]
struct NativeBoxWindows {
    drag_handle: HWND,
    panel: HWND,
}

pub fn run(app: AppHandle, receiver: Receiver<NativeDesktopCommand>) {
    log_native_desktop(
        "controller.run",
        "Explorer-native desktop controller thread started",
    );
    let mut controller = WindowsNativeDesktopController {
        app,
        hwnd: None,
        box_windows: HashMap::new(),
        explorer_icons: ExplorerIconBridge::default(),
        last_explorer_sync: Instant::now(),
        model: NativeDesktopModel::default(),
    };
    let controller_ptr = &mut controller as *mut WindowsNativeDesktopController;

    unsafe {
        GLOBAL_CONTROLLER = controller_ptr;
        let co_result = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        log_native_desktop(
            "controller.run",
            format!("CoInitializeEx result={co_result:?}"),
        );
    }

    let mut should_exit = false;
    while !should_exit {
        should_exit = controller.drain_commands(&receiver);
        if should_exit {
            break;
        }
        if controller.dispatch_pending_messages() {
            log_native_desktop("controller.run", "WM_QUIT received from message queue");
            break;
        }
        controller.tick();
        match receiver.recv_timeout(MESSAGE_IDLE_TIMEOUT) {
            Ok(command) => should_exit = controller.handle_command(command),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                log_native_desktop("controller.run", "command channel disconnected");
                break;
            }
        }
    }

    log_native_desktop("controller.run", "controller cleanup begin");
    controller.destroy_layer();
    controller.explorer_icons.reset();
    let _ = set_native_desktop_icons_hidden(false);
    unsafe {
        GLOBAL_CONTROLLER = ptr::null_mut();
    }
    log_native_desktop("controller.run", "controller cleanup end");
}

static mut GLOBAL_CONTROLLER: *mut WindowsNativeDesktopController = ptr::null_mut();

impl WindowsNativeDesktopController {
    fn drain_commands(&mut self, receiver: &Receiver<NativeDesktopCommand>) -> bool {
        let mut should_exit = false;
        while let Ok(command) = receiver.try_recv() {
            log_native_desktop("controller.drain_commands", "drained queued command");
            should_exit = self.handle_command(command);
            if should_exit {
                break;
            }
        }

        should_exit
    }

    fn dispatch_pending_messages(&mut self) -> bool {
        let mut should_quit = false;
        loop {
            let mut message = MSG::default();
            let has_message =
                unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() };
            if !has_message {
                break;
            }
            if message.message == WM_QUIT {
                should_quit = true;
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        should_quit
    }

    fn handle_command(&mut self, command: NativeDesktopCommand) -> bool {
        log_native_desktop(
            "controller.handle_command",
            describe_native_desktop_command(&command),
        );
        match command {
            NativeDesktopCommand::SetEnabled(enabled) => {
                if let Err(error) = self.set_enabled(enabled) {
                    log_native_desktop(
                        "controller.handle_command",
                        format!("SetEnabled failed: {error}"),
                    );
                    self.emit_error(error);
                    self.model.enabled = false;
                    self.destroy_layer();
                    let _ = set_native_desktop_icons_hidden(false);
                }
                false
            }
            NativeDesktopCommand::SyncState(state) => {
                self.model.apply_state(state);
                if self.model.enabled {
                    if let Err(error) = self.rebuild_native_surfaces() {
                        log_native_desktop(
                            "controller.handle_command",
                            format!("SyncState rebuild failed: {error}"),
                        );
                        self.emit_error(error);
                    }
                }
                false
            }
            NativeDesktopCommand::Refresh => {
                if self.model.enabled {
                    log_native_desktop("controller.handle_command", "refreshing native layer");
                    self.explorer_icons.reset();
                    self.ensure_layer().ok();
                    self.rebuild_native_surfaces().ok();
                }
                false
            }
            NativeDesktopCommand::Shutdown => {
                log_native_desktop("controller.handle_command", "shutdown command received");
                true
            }
        }
    }

    fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        log_native_desktop("controller.set_enabled", format!("enter enabled={enabled}"));
        if enabled {
            self.model.enabled = true;
            // A 路线的核心约束：Explorer 原生桌面图标层不能被隐藏，右键、框选、多选等交互都交回系统。
            if let Err(error) = set_native_desktop_icons_hidden(false) {
                log_native_desktop(
                    "controller.set_enabled",
                    format!("show Explorer icons skipped: {error}"),
                );
            }
            self.ensure_layer()?;
            self.rebuild_native_surfaces()?;
            log_native_desktop("controller.set_enabled", "enabled path completed");
            return Ok(());
        }

        self.model.enabled = false;
        log_native_desktop("controller.set_enabled", "disabling native layer");
        self.destroy_layer();
        self.explorer_icons.reset();
        // 关闭接管时以恢复 Explorer 为目标；Explorer 刚重启时句柄可能暂不可定位，不能因此阻断设置页关闭开关。
        if let Err(error) = set_native_desktop_icons_hidden(false) {
            log_native_desktop(
                "controller.set_enabled",
                format!("show Explorer icons skipped on disable: {error}"),
            );
        }
        Ok(())
    }

    fn ensure_layer(&mut self) -> Result<(), String> {
        log_native_desktop("controller.ensure_layer", "enter");
        if let Some(hwnd) = self.hwnd {
            if is_live_window(hwnd) {
                self.resize_layer(hwnd);
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                }
                return Ok(());
            }
        }

        self.hwnd = None;
        self.box_windows.clear();
        unsafe {
            register_layer_class()?;
            register_box_class()?;
        }
        let hwnd = unsafe { create_layer_window()? };
        if let Some(parent) = find_desktop_backdrop_parent() {
            unsafe {
                convert_layer_to_desktop_child(hwnd, parent);
            }
        } else {
            log_native_desktop(
                "controller.ensure_layer",
                "desktop backdrop parent not found, using popup fallback",
            );
        }

        self.hwnd = Some(hwnd);
        self.resize_layer(hwnd);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        log_native_desktop("controller.ensure_layer", "new layer ready");
        Ok(())
    }

    fn destroy_layer(&mut self) {
        log_native_desktop(
            "controller.destroy_layer",
            format!("destroying boxes={}", self.box_windows.len()),
        );
        self.box_windows.clear();
        if let Some(hwnd) = self.hwnd.take() {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
        log_native_desktop("controller.destroy_layer", "destroy complete");
    }

    fn resize_layer(&mut self, hwnd: HWND) {
        let (x, y, width, height, parent) = resolve_layer_rect(hwnd);
        log_native_desktop(
            "controller.resize_layer",
            format!(
                "hwnd={} parent={} pos=({}, {}) size={}x{}",
                hwnd_value(hwnd),
                parent
                    .map(describe_window)
                    .unwrap_or_else(|| "none".to_string()),
                x,
                y,
                width,
                height
            ),
        );
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
        log_window_snapshot("controller.resize_layer.after", hwnd);
    }

    fn rebuild_native_surfaces(&mut self) -> Result<(), String> {
        log_native_desktop("controller.rebuild_native_surfaces", "enter");
        self.ensure_layer()?;
        let Some(state) = self.model.state().cloned() else {
            log_native_desktop("controller.rebuild_native_surfaces", "skip: no state");
            return Ok(());
        };

        self.sync_box_windows(&state)?;
        self.sync_explorer_icons(&state);
        log_native_desktop("controller.rebuild_native_surfaces", "completed");
        Ok(())
    }

    fn sync_box_windows(&mut self, state: &NativeDesktopState) -> Result<(), String> {
        let live_box_ids: HashSet<String> =
            state.boxes.iter().map(|item| item.id.clone()).collect();
        let removed_box_ids = self
            .box_windows
            .keys()
            .filter(|box_id| !live_box_ids.contains(*box_id))
            .cloned()
            .collect::<Vec<_>>();
        for box_id in removed_box_ids {
            if let Some(windows) = self.box_windows.remove(&box_id) {
                unsafe {
                    let _ = DestroyWindow(windows.drag_handle);
                    let _ = DestroyWindow(windows.panel);
                }
            }
        }

        for box_state in &state.boxes {
            if !self.box_windows.contains_key(&box_state.id) {
                let windows = unsafe { create_box_windows(box_state, &state.display)? };
                self.box_windows.insert(box_state.id.clone(), windows);
            }
            if let Some(windows) = self.box_windows.get(&box_state.id).copied() {
                layout_box_windows(windows, box_state, &state.display);
                unsafe {
                    let _ = InvalidateRect(Some(windows.panel), None, true);
                    let _ = UpdateWindow(windows.panel);
                }
            }
        }

        Ok(())
    }

    fn sync_explorer_icons(&mut self, state: &NativeDesktopState) {
        if let Err(error) = self.explorer_icons.sync_state(&self.app, state) {
            log_native_desktop(
                "controller.sync_explorer_icons",
                format!("sync skipped: {error}"),
            );
        }
    }

    fn tick(&mut self) {
        if !self.model.enabled || self.last_explorer_sync.elapsed() < EXPLORER_SYNC_INTERVAL {
            return;
        }
        self.last_explorer_sync = Instant::now();
        if let Some(state) = self.model.state().cloned() {
            self.sync_explorer_icons(&state);
        }
    }

    fn emit_error(&self, message: String) {
        let _ = self.app.emit_to(
            "main",
            NATIVE_DESKTOP_ERROR_EVENT,
            NativeDesktopError { message },
        );
    }

    fn box_state_by_hwnd(&self, hwnd: HWND) -> Option<NativeBoxState> {
        let box_id = self.box_id_by_hwnd(hwnd)?;
        self.model
            .state()
            .and_then(|state| state.boxes.iter().find(|box_state| box_state.id == box_id))
            .cloned()
    }

    fn box_id_by_hwnd(&self, hwnd: HWND) -> Option<String> {
        self.box_windows.iter().find_map(|(box_id, box_windows)| {
            (box_windows.panel == hwnd || box_windows.drag_handle == hwnd).then(|| box_id.clone())
        })
    }

    /// 系统标题栏拖动结束后同步运行时和前端持久化源，避免下次刷新把 Box 拉回旧位置。
    fn handle_box_move_finished(&mut self, hwnd: HWND) {
        let Some((box_id, windows)) = self.box_windows.iter().find_map(|(box_id, windows)| {
            (windows.drag_handle == hwnd).then(|| (box_id.clone(), *windows))
        }) else {
            return;
        };
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(windows.panel, &mut rect) }.is_err() {
            log_native_desktop(
                "controller.handle_box_move_finished",
                format!(
                    "skip: GetWindowRect failed hwnd={}",
                    hwnd_value(windows.panel)
                ),
            );
            return;
        }

        self.model.update_box_position(&box_id, rect.left, rect.top);
        log_native_desktop(
            "controller.handle_box_move_finished",
            format!("box_id={box_id} x={} y={}", rect.left, rect.top),
        );
        let _ = self.app.emit_to(
            "main",
            NATIVE_DESKTOP_BOX_MOVED_EVENT,
            NativeDesktopBoxMoved {
                box_id,
                x: rect.left,
                y: rect.top,
            },
        );
    }

    /// 系统拖动只作用于标题句柄窗口，实时把视觉面板贴回同一个 Box 矩形。
    fn sync_panel_position_from_drag_handle(&self, hwnd: HWND) {
        let Some(windows) = self
            .box_windows
            .values()
            .find(|windows| windows.drag_handle == hwnd)
            .copied()
        else {
            return;
        };
        let Some(box_state) = self.box_state_by_hwnd(hwnd) else {
            return;
        };
        let mut handle_rect = RECT::default();
        if unsafe { GetWindowRect(windows.drag_handle, &mut handle_rect) }.is_err() {
            return;
        }
        let (_, _, width, height) = resolve_box_screen_rect(&box_state);
        let panel_y = if box_state.title_position == NativeBoxTitlePosition::Bottom {
            handle_rect.top - height + BOX_TITLE_HEIGHT
        } else {
            handle_rect.top
        };
        unsafe {
            let _ = SetWindowPos(
                windows.panel,
                Some(HWND_TOP),
                handle_rect.left,
                panel_y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(Some(windows.panel), None, true);
        }
    }

    /// 根据面板窗口定位当前 Box 的绘制输入，Box 内项目必须来自 Dasktop 状态而不是 Explorer 坐标。
    fn box_render_model_by_hwnd(&self, hwnd: HWND) -> Option<NativeBoxRenderModel> {
        let box_id = self.box_id_by_hwnd(hwnd)?;
        let state = self.model.state()?;
        let box_state = state
            .boxes
            .iter()
            .find(|box_state| box_state.id == box_id)?
            .clone();
        let items_by_key = state
            .items
            .iter()
            .map(|item| (normalize_item_key(&item.path), item))
            .collect::<HashMap<_, _>>();
        let mut items = state
            .box_items
            .iter()
            .filter(|box_item| box_item.box_id == box_id)
            .filter_map(|box_item| {
                items_by_key
                    .get(&normalize_item_key(&box_item.item_path))
                    .map(|item| NativeBoxRenderItem {
                        name: item.name.clone(),
                        icon_data_url: item.icon_data_url.clone(),
                        order_index: box_item.order_index,
                    })
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.order_index);

        Some(NativeBoxRenderModel {
            box_state,
            display: state.display.clone(),
            items,
        })
    }
}

/// 原生 Box 面板绘制所需的完整数据，避免绘制函数回头依赖 Explorer 的桌面图标坐标。
struct NativeBoxRenderModel {
    box_state: NativeBoxState,
    display: NativeDesktopDisplaySettings,
    items: Vec<NativeBoxRenderItem>,
}

/// Box 内部项目的轻量绘制模型，排序和图标数据都来自前端同步状态。
struct NativeBoxRenderItem {
    name: String,
    icon_data_url: Option<String>,
    order_index: i32,
}

fn hit_test_box(boxes: &[NativeBoxState], screen_x: i32, screen_y: i32) -> Option<String> {
    boxes.iter().find_map(|box_state| {
        let height = if box_state.collapsed {
            BOX_TITLE_HEIGHT
        } else {
            box_state.height
        };
        let inside = screen_x >= box_state.x
            && screen_y >= box_state.y
            && screen_x <= box_state.x + box_state.width
            && screen_y <= box_state.y + height;
        inside.then(|| box_state.id.clone())
    })
}

/// 把已经归属 Box 的 Explorer 图标停放到虚拟屏幕外，避免用户看到“背景图标跟着 Box 跑”。
fn explorer_icon_parking_origin() -> (i32, i32) {
    (
        unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) } - EXPLORER_PARKING_MARGIN,
        unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) } - EXPLORER_PARKING_MARGIN,
    )
}

/// 判断 Explorer 图标是否已经处于停放区；停放区内的坐标不应触发“拖回桌面”事件。
fn is_parked_icon_position(screen_x: i32, screen_y: i32) -> bool {
    let (parking_x, parking_y) = explorer_icon_parking_origin();
    screen_x >= parking_x
        && screen_x <= parking_x + 1024
        && screen_y >= parking_y
        && screen_y <= parking_y + 1024
}

fn is_live_window(hwnd: HWND) -> bool {
    !hwnd.is_invalid() && unsafe { IsWindow(Some(hwnd)).as_bool() }
}

fn find_desktop_backdrop_parent() -> Option<HWND> {
    log_native_desktop("win32.find_desktop_backdrop_parent", "begin");
    let parent = find_shell_def_view().or_else(|| {
        // Explorer 重建桌面层级时 `SHELLDLL_DefView` 可能短暂不可见，WorkerW 只作为兜底。
        ensure_workerw_available();
        find_workerw_behind_shell_view().or_else(find_progman_window)
    });
    log_native_desktop(
        "win32.find_desktop_backdrop_parent",
        format!(
            "result={}",
            parent
                .map(describe_window)
                .unwrap_or_else(|| "none".to_string())
        ),
    );
    parent
}

fn find_desktop_list_view() -> Option<HWND> {
    let def_view = find_shell_def_view()?;
    unsafe {
        FindWindowExW(
            Some(def_view),
            None,
            PCWSTR(to_wide("SysListView32").as_ptr()),
            PCWSTR::null(),
        )
        .ok()
    }
}

fn find_shell_def_view() -> Option<HWND> {
    if let Some(progman) = find_progman_window() {
        if let Some(def_view) = find_child_class(progman, "SHELLDLL_DefView") {
            return Some(def_view);
        }
    }

    let mut search = DesktopParentSearch::default();
    unsafe {
        let _ = EnumWindows(
            Some(enum_shell_def_view_proc),
            LPARAM((&mut search as *mut DesktopParentSearch) as isize),
        );
    }
    search.def_view
}

fn resolve_layer_rect(hwnd: HWND) -> (i32, i32, i32, i32, Option<HWND>) {
    if let Ok(parent) = unsafe { GetParent(hwnd) } {
        let mut rect = RECT::default();
        if unsafe { GetClientRect(parent, &mut rect) }.is_ok() {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width > 0 && height > 0 {
                return (0, 0, width, height, Some(parent));
            }
        }
    }

    (
        unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
        unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
        unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) },
        unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) },
        None,
    )
}

fn ensure_workerw_available() {
    let Some(progman) = find_progman_window() else {
        return;
    };
    let mut result = 0usize;
    unsafe {
        let _ = SendMessageTimeoutW(
            progman,
            DESKTOP_SPAWN_WORKERW_MESSAGE,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            Some(&mut result),
        );
    }
}

fn find_workerw_behind_shell_view() -> Option<HWND> {
    let mut search = DesktopParentSearch::default();
    unsafe {
        let _ = EnumWindows(
            Some(enum_desktop_parent_proc),
            LPARAM((&mut search as *mut DesktopParentSearch) as isize),
        );
    }
    search.workerw
}

fn find_progman_window() -> Option<HWND> {
    unsafe { FindWindowW(PCWSTR(to_wide("Progman").as_ptr()), PCWSTR::null()).ok() }
}

unsafe extern "system" fn enum_desktop_parent_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam.0 as *mut DesktopParentSearch);
    if find_child_class(hwnd, "SHELLDLL_DefView").is_some() {
        if let Some(workerw) = next_workerw(hwnd) {
            search.workerw = Some(workerw);
            return BOOL(0);
        }
    }
    BOOL(1)
}

unsafe extern "system" fn enum_shell_def_view_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam.0 as *mut DesktopParentSearch);
    if let Some(def_view) = find_child_class(hwnd, "SHELLDLL_DefView") {
        search.def_view = Some(def_view);
        return BOOL(0);
    }
    BOOL(1)
}

fn find_child_class(hwnd: HWND, class_name: &str) -> Option<HWND> {
    unsafe {
        FindWindowExW(
            Some(hwnd),
            None,
            PCWSTR(to_wide(class_name).as_ptr()),
            PCWSTR::null(),
        )
        .ok()
    }
}

fn next_workerw(hwnd: HWND) -> Option<HWND> {
    let mut next = unsafe { GetWindow(hwnd, GW_HWNDNEXT).ok() };
    while let Some(candidate) = next {
        if window_class_name(candidate) == "WorkerW" {
            return Some(candidate);
        }
        next = unsafe { GetWindow(candidate, GW_HWNDNEXT).ok() };
    }
    None
}

#[derive(Default)]
struct DesktopParentSearch {
    def_view: Option<HWND>,
    workerw: Option<HWND>,
}

unsafe fn convert_layer_to_desktop_child(hwnd: HWND, parent: HWND) {
    let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    let next_style = (style & !WS_POPUP.0) | WS_CHILD.0 | WS_VISIBLE.0 | WS_CLIPCHILDREN.0;
    let old_style = SetWindowLongW(hwnd, GWL_STYLE, next_style as i32);
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let next_ex_style = ex_style | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0;
    let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, next_ex_style as i32);
    let parent_result = SetParent(hwnd, Some(parent));
    log_native_desktop(
        "win32.convert_layer_to_desktop_child",
        format!(
            "hwnd={} parent={} old_style=0x{old_style:x} set_parent={parent_result:?}",
            describe_window(hwnd),
            describe_window(parent)
        ),
    );
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_TOP),
        0,
        0,
        1,
        1,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    log_window_snapshot("win32.convert_layer_to_desktop_child.after", hwnd);
}

fn window_class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 128];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// 输出窗口类名、父窗口和可见性，专门用于定位桌面壳层级问题。
fn describe_window(hwnd: HWND) -> String {
    let class_name = window_class_name(hwnd);
    let parent = unsafe { GetParent(hwnd).ok() }
        .map(hwnd_value)
        .map(|value| format!("0x{value:x}"))
        .unwrap_or_else(|| "none".to_string());
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) as u32 };
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 };
    let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    format!(
        "0x{:x}/class={}/parent={}/visible={}/style=0x{:x}/ex=0x{:x}",
        hwnd_value(hwnd),
        class_name,
        parent,
        visible,
        style,
        ex_style
    )
}

/// 记录窗口客户区、屏幕矩形和相邻 z-order，帮助判断控件是否被桌面壳或兄弟窗口遮挡。
fn log_window_snapshot(stage: &str, hwnd: HWND) {
    unsafe {
        let mut client = RECT::default();
        let client_text = if GetClientRect(hwnd, &mut client).is_ok() {
            format!(
                "{}x{}",
                client.right - client.left,
                client.bottom - client.top
            )
        } else {
            "client-error".to_string()
        };
        let mut window = RECT::default();
        let window_text = if GetWindowRect(hwnd, &mut window).is_ok() {
            format!(
                "({}, {})-({}, {})",
                window.left, window.top, window.right, window.bottom
            )
        } else {
            "window-error".to_string()
        };
        let child = GetWindow(hwnd, GW_CHILD)
            .ok()
            .map(describe_window)
            .unwrap_or_else(|| "none".to_string());
        let previous = GetWindow(hwnd, GW_HWNDPREV)
            .ok()
            .map(describe_window)
            .unwrap_or_else(|| "none".to_string());
        let next = GetWindow(hwnd, GW_HWNDNEXT)
            .ok()
            .map(describe_window)
            .unwrap_or_else(|| "none".to_string());

        log_native_desktop(
            stage,
            format!(
                "window={} client={} rect={} child={} prev={} next={}",
                describe_window(hwnd),
                client_text,
                window_text,
                child,
                previous,
                next
            ),
        );
    }
}

/// 统一输出 Win32 句柄的十六进制值，便于和系统事件日志或 Spy++ 结果交叉核对。
fn hwnd_value(hwnd: HWND) -> usize {
    hwnd.0 as usize
}

/// windows-rs 0.61 把 Win32 消息参数建模为 Option，统一封装能避免调用点散落样板代码。
unsafe fn send_message(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    SendMessageW(hwnd, message, Some(wparam), Some(lparam))
}
