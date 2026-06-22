//! Explorer 桌面 ListView 桥接，负责读取系统图标坐标、检测拖入 Box，并停放已归属项目。

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};
use windows::Win32::UI::Controls::{
    LVIF_TEXT, LVITEMW, LVM_GETITEMCOUNT, LVM_GETITEMPOSITION, LVM_GETITEMTEXTW,
    LVM_SETITEMPOSITION32,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use super::display_name::{build_display_name_map, resolve_item_path_by_display_name};
use super::{
    explorer_icon_parking_origin, find_desktop_list_view, hit_test_box, hwnd_value, is_live_window,
    is_parked_icon_position, send_message, REMOTE_TEXT_CHARS,
};
use crate::domain::desktop::NativeDesktopState;
use crate::infrastructure::desktop::log_native_desktop;

use super::super::events::{
    NativeDesktopDropToBox, NativeDesktopDropToDesktop, NATIVE_DESKTOP_DROP_TO_BOX_EVENT,
    NATIVE_DESKTOP_DROP_TO_DESKTOP_EVENT,
};
use super::super::model::normalize_item_key;

const PENDING_BOX_ASSIGNMENT_TIMEOUT: Duration = Duration::from_secs(6);

/// Explorer 图标桥只读写系统桌面 ListView 坐标，图标绘制、选择、右键和拖拽仍由 Explorer 负责。
#[derive(Default)]
pub(super) struct ExplorerIconBridge {
    desktop_list: Option<HWND>,
    process: Option<HANDLE>,
    remote_point: *mut c_void,
    remote_text: *mut c_void,
    remote_item: *mut c_void,
    observed_containers: HashMap<String, Option<String>>,
    pending_box_assignments: HashMap<String, PendingBoxAssignment>,
    parked_restore_points: HashMap<String, ParkedIconRestorePoint>,
}

impl ExplorerIconBridge {
    pub(super) fn sync_state(
        &mut self,
        app: &AppHandle,
        state: &NativeDesktopState,
    ) -> Result<(), String> {
        self.ensure_attached()?;
        self.reconcile_pending_assignments(state);
        let explorer_items = self.read_explorer_items(state)?;
        self.emit_mapping_changes(app, &explorer_items, state);
        self.park_assigned_items(&explorer_items, state)?;
        Ok(())
    }

    fn ensure_attached(&mut self) -> Result<(), String> {
        if let Some(list) = self.desktop_list {
            if is_live_window(list) && self.process.is_some() {
                return Ok(());
            }
        }

        self.reset();
        let list = find_desktop_list_view()
            .ok_or_else(|| "无法定位 Windows 桌面图标窗口，稍后会继续重试".to_string())?;
        let mut process_id = 0u32;
        unsafe {
            GetWindowThreadProcessId(list, Some(&mut process_id));
        }
        if process_id == 0 {
            return Err("无法读取 Explorer 桌面进程 ID".to_string());
        }

        let process = unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION
                    | PROCESS_VM_OPERATION
                    | PROCESS_VM_READ
                    | PROCESS_VM_WRITE,
                false,
                process_id,
            )
        }
        .map_err(|error| format!("无法打开 Explorer 桌面进程：{error}"))?;

        let remote_point = unsafe {
            VirtualAllocEx(
                process,
                None,
                size_of::<POINT>(),
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        let remote_text = unsafe {
            VirtualAllocEx(
                process,
                None,
                REMOTE_TEXT_CHARS * size_of::<u16>(),
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        let remote_item = unsafe {
            VirtualAllocEx(
                process,
                None,
                size_of::<LVITEMW>(),
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        if remote_point.is_null() || remote_text.is_null() || remote_item.is_null() {
            self.process = Some(process);
            self.remote_point = remote_point;
            self.remote_text = remote_text;
            self.remote_item = remote_item;
            self.reset();
            return Err("无法在 Explorer 进程分配图标同步缓冲区".to_string());
        }

        self.desktop_list = Some(list);
        self.process = Some(process);
        self.remote_point = remote_point;
        self.remote_text = remote_text;
        self.remote_item = remote_item;
        log_native_desktop(
            "explorer_bridge.ensure_attached",
            format!(
                "attached list={} process_id={} point={remote_point:p} text={remote_text:p} item={remote_item:p}",
                hwnd_value(list),
                process_id
            ),
        );
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        self.restore_all_parked_items();
        if let Some(process) = self.process.take() {
            unsafe {
                if !self.remote_point.is_null() {
                    let _ = VirtualFreeEx(process, self.remote_point, 0, MEM_RELEASE);
                }
                if !self.remote_text.is_null() {
                    let _ = VirtualFreeEx(process, self.remote_text, 0, MEM_RELEASE);
                }
                if !self.remote_item.is_null() {
                    let _ = VirtualFreeEx(process, self.remote_item, 0, MEM_RELEASE);
                }
                let _ = CloseHandle(process);
            }
        }
        self.desktop_list = None;
        self.remote_point = ptr::null_mut();
        self.remote_text = ptr::null_mut();
        self.remote_item = ptr::null_mut();
        self.observed_containers.clear();
        self.pending_box_assignments.clear();
        self.parked_restore_points.clear();
    }

    fn read_explorer_items(
        &mut self,
        state: &NativeDesktopState,
    ) -> Result<Vec<ExplorerDesktopItem>, String> {
        let list = self
            .desktop_list
            .filter(|hwnd| is_live_window(*hwnd))
            .ok_or_else(|| "Explorer 桌面图标窗口已失效".to_string())?;
        let display_name_map = build_display_name_map(&state.items);
        let count = unsafe { send_message(list, LVM_GETITEMCOUNT, WPARAM(0), LPARAM(0)).0 };
        if count < 0 {
            return Err("Explorer 桌面图标数量读取失败".to_string());
        }

        let mut items = Vec::new();
        for index in 0..count {
            let display_name = self.read_item_text(list, index as i32)?;
            let Some(item_path) =
                resolve_item_path_by_display_name(&display_name_map, &display_name)
            else {
                continue;
            };
            let Some((screen_x, screen_y)) = self.read_item_position(list, index as i32)? else {
                continue;
            };
            items.push(ExplorerDesktopItem {
                index: index as i32,
                item_path,
                screen_x,
                screen_y,
            });
        }

        Ok(items)
    }

    fn read_item_text(&self, list: HWND, index: i32) -> Result<String, String> {
        let Some(process) = self.process else {
            return Err("Explorer 进程句柄未初始化".to_string());
        };
        let mut item = LVITEMW {
            mask: LVIF_TEXT,
            iItem: index,
            iSubItem: 0,
            pszText: PWSTR(self.remote_text as *mut u16),
            cchTextMax: REMOTE_TEXT_CHARS as i32,
            ..LVITEMW::default()
        };
        unsafe {
            WriteProcessMemory(
                process,
                self.remote_item,
                (&mut item as *mut LVITEMW).cast(),
                size_of::<LVITEMW>(),
                None,
            )
            .map_err(|error| format!("无法写入 Explorer LVITEMW：{error}"))?;
            let _ = send_message(
                list,
                LVM_GETITEMTEXTW,
                WPARAM(index as usize),
                LPARAM(self.remote_item as isize),
            );
            let mut buffer = vec![0u16; REMOTE_TEXT_CHARS];
            ReadProcessMemory(
                process,
                self.remote_text,
                buffer.as_mut_ptr().cast(),
                buffer.len() * size_of::<u16>(),
                None,
            )
            .map_err(|error| format!("无法读取 Explorer 图标文本：{error}"))?;
            let length = buffer
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(buffer.len());
            Ok(String::from_utf16_lossy(&buffer[..length]))
        }
    }

    fn read_item_position(&self, list: HWND, index: i32) -> Result<Option<(i32, i32)>, String> {
        let Some(process) = self.process else {
            return Err("Explorer 进程句柄未初始化".to_string());
        };
        let result = unsafe {
            send_message(
                list,
                LVM_GETITEMPOSITION,
                WPARAM(index as usize),
                LPARAM(self.remote_point as isize),
            )
        };
        if result.0 == 0 {
            return Ok(None);
        }

        let mut point = POINT::default();
        unsafe {
            ReadProcessMemory(
                process,
                self.remote_point,
                (&mut point as *mut POINT).cast(),
                size_of::<POINT>(),
                None,
            )
            .map_err(|error| format!("无法读取 Explorer 图标坐标：{error}"))?;
            let _ = ClientToScreen(list, &mut point);
        }
        Ok(Some((point.x, point.y)))
    }

    fn park_assigned_items(
        &mut self,
        explorer_items: &[ExplorerDesktopItem],
        state: &NativeDesktopState,
    ) -> Result<(), String> {
        let list = self
            .desktop_list
            .filter(|hwnd| is_live_window(*hwnd))
            .ok_or_else(|| "Explorer 桌面图标窗口已失效".to_string())?;
        let assigned_box_by_item = self.effective_assigned_box_by_item(state);
        let (parking_x, parking_y) = explorer_icon_parking_origin();

        self.restore_unassigned_parked_items(list, &assigned_box_by_item);

        for explorer_item in explorer_items {
            let item_key = normalize_item_key(&explorer_item.item_path);
            let Some(box_id) = assigned_box_by_item.get(&item_key) else {
                continue;
            };
            let offset = (explorer_item.index % 97).max(0) * 6;
            let target = (parking_x + offset, parking_y + offset);
            if is_parked_icon_position(explorer_item.screen_x, explorer_item.screen_y) {
                continue;
            }
            log_native_desktop(
                "explorer_bridge.park_assigned_items",
                format!(
                    "park item={} box_id={} from=({}, {}) to=({}, {}) index={}",
                    explorer_item.item_path,
                    box_id,
                    explorer_item.screen_x,
                    explorer_item.screen_y,
                    target.0,
                    target.1,
                    explorer_item.index
                ),
            );
            self.set_item_position_from_screen(list, explorer_item.index, target.0, target.1)?;
            self.parked_restore_points
                .entry(item_key.clone())
                .or_insert(ParkedIconRestorePoint {
                    index: explorer_item.index,
                    screen_x: explorer_item.screen_x,
                    screen_y: explorer_item.screen_y,
                });
            self.observed_containers
                .insert(item_key, Some(box_id.clone()));
        }

        Ok(())
    }

    fn set_item_position_from_screen(
        &self,
        list: HWND,
        index: i32,
        screen_x: i32,
        screen_y: i32,
    ) -> Result<(), String> {
        let Some(process) = self.process else {
            return Err("Explorer 进程句柄未初始化".to_string());
        };
        let mut point = POINT {
            x: screen_x,
            y: screen_y,
        };
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(list, &mut point);
            WriteProcessMemory(
                process,
                self.remote_point,
                (&mut point as *mut POINT).cast(),
                size_of::<POINT>(),
                None,
            )
            .map_err(|error| format!("无法写入 Explorer 图标目标坐标：{error}"))?;
            let _ = send_message(
                list,
                LVM_SETITEMPOSITION32,
                WPARAM(index as usize),
                LPARAM(self.remote_point as isize),
            );
        }
        Ok(())
    }

    fn emit_mapping_changes(
        &mut self,
        app: &AppHandle,
        explorer_items: &[ExplorerDesktopItem],
        state: &NativeDesktopState,
    ) {
        let persisted_assigned_box_by_item = Self::persisted_assigned_box_by_item(state);
        let assigned_box_by_item = self.effective_assigned_box_by_item(state);
        let boxes = &state.boxes;

        for explorer_item in explorer_items {
            let item_key = normalize_item_key(&explorer_item.item_path);
            if is_parked_icon_position(explorer_item.screen_x, explorer_item.screen_y) {
                if let Some(assigned_box_id) = assigned_box_by_item.get(&item_key) {
                    self.observed_containers
                        .insert(item_key, Some(assigned_box_id.clone()));
                }
                continue;
            }
            let actual_box_id = hit_test_box(boxes, explorer_item.screen_x, explorer_item.screen_y);
            let assigned_box_id = assigned_box_by_item.get(&item_key).cloned();
            let persisted_assigned_box_id = persisted_assigned_box_by_item.get(&item_key).cloned();
            let pending_box_id = self
                .pending_box_assignments
                .get(&item_key)
                .map(|assignment| assignment.box_id.clone());
            let previous_box_id = self.observed_containers.get(&item_key).cloned();

            if pending_box_id.is_some() && actual_box_id.is_none() {
                log_native_desktop(
                    "explorer_bridge.emit_mapping_changes",
                    format!(
                        "hold pending item={} box_id={} at desktop=({}, {})",
                        explorer_item.item_path,
                        pending_box_id.as_deref().unwrap_or_default(),
                        explorer_item.screen_x,
                        explorer_item.screen_y
                    ),
                );
                self.observed_containers
                    .insert(item_key, pending_box_id.clone());
                continue;
            }
            if actual_box_id.is_none() {
                if let Some(box_id) = assigned_box_id.clone() {
                    // 已进入 Box 的项目由 Dasktop 原生 Box 绘制，Explorer 原图标只作为拖入检测来源。
                    // 因此这里把可见桌面坐标视为待停放状态，而不是“拖回桌面”的用户意图。
                    log_native_desktop(
                        "explorer_bridge.emit_mapping_changes",
                        format!(
                            "hold assigned item={} box_id={} at desktop=({}, {})",
                            explorer_item.item_path,
                            box_id,
                            explorer_item.screen_x,
                            explorer_item.screen_y
                        ),
                    );
                    self.observed_containers.insert(item_key, Some(box_id));
                    continue;
                }
            }

            let observed_container_changed = previous_box_id.as_ref() != Some(&actual_box_id);
            let mapping_requires_update = actual_box_id != assigned_box_id
                && (actual_box_id.is_some() || assigned_box_id.is_some());
            // Explorer 拖放只改变桌面 ListView 坐标，Dasktop 必须把坐标命中结果补写成 Box 映射。
            // 首次读取到的图标可能已经位于 Box 内，不能要求 previous 存在，否则第一次拖入会被当作基线吞掉。
            if observed_container_changed && mapping_requires_update {
                match actual_box_id.clone() {
                    Some(box_id) => {
                        log_native_desktop(
                            "explorer_bridge.emit_mapping_changes",
                            format!(
                                "emit drop-to-box item={} box_id={} at=({}, {}) assigned={:?}",
                                explorer_item.item_path,
                                box_id,
                                explorer_item.screen_x,
                                explorer_item.screen_y,
                                assigned_box_id
                            ),
                        );
                        self.pending_box_assignments.insert(
                            item_key.clone(),
                            PendingBoxAssignment {
                                box_id: box_id.clone(),
                                item_path: explorer_item.item_path.clone(),
                                created_at: Instant::now(),
                            },
                        );
                        let _ = app.emit_to(
                            "main",
                            NATIVE_DESKTOP_DROP_TO_BOX_EVENT,
                            NativeDesktopDropToBox {
                                box_id,
                                item_path: explorer_item.item_path.clone(),
                                screen_x: explorer_item.screen_x,
                                screen_y: explorer_item.screen_y,
                            },
                        );
                    }
                    None => {
                        if previous_box_id
                            .as_ref()
                            .and_then(|value| value.as_ref())
                            .is_some()
                            && assigned_box_id.is_some()
                        {
                            log_native_desktop(
                                "explorer_bridge.emit_mapping_changes",
                                format!(
                                    "emit drop-to-desktop item={} source={:?} at=({}, {})",
                                    explorer_item.item_path,
                                    assigned_box_id,
                                    explorer_item.screen_x,
                                    explorer_item.screen_y
                                ),
                            );
                            self.pending_box_assignments.remove(&item_key);
                            let _ = app.emit_to(
                                "main",
                                NATIVE_DESKTOP_DROP_TO_DESKTOP_EVENT,
                                NativeDesktopDropToDesktop {
                                    source_box_id: persisted_assigned_box_id.or(assigned_box_id),
                                    item_path: explorer_item.item_path.clone(),
                                    x: explorer_item.screen_x,
                                    y: explorer_item.screen_y,
                                },
                            );
                        }
                    }
                }
            }

            self.observed_containers.insert(item_key, actual_box_id);
        }
    }

    /// 前端状态一旦确认或改写 Box 映射，native 层的临时归属就可以让位给持久化状态。
    fn reconcile_pending_assignments(&mut self, state: &NativeDesktopState) {
        let persisted_assigned_box_by_item = Self::persisted_assigned_box_by_item(state);
        let known_item_keys = state
            .items
            .iter()
            .map(|item| normalize_item_key(&item.path))
            .collect::<HashSet<_>>();
        let now = Instant::now();
        let mut resolved_item_keys = Vec::new();

        for (item_key, assignment) in &self.pending_box_assignments {
            if !known_item_keys.contains(item_key) {
                log_native_desktop(
                    "explorer_bridge.reconcile_pending_assignments",
                    format!(
                        "drop pending unknown item={} box_id={}",
                        assignment.item_path, assignment.box_id
                    ),
                );
                resolved_item_keys.push(item_key.clone());
                continue;
            }
            if let Some(persisted_box_id) = persisted_assigned_box_by_item.get(item_key) {
                log_native_desktop(
                    "explorer_bridge.reconcile_pending_assignments",
                    format!(
                        "confirm pending item={} pending_box={} persisted_box={}",
                        assignment.item_path, assignment.box_id, persisted_box_id
                    ),
                );
                resolved_item_keys.push(item_key.clone());
                continue;
            }
            if now.duration_since(assignment.created_at) > PENDING_BOX_ASSIGNMENT_TIMEOUT {
                log_native_desktop(
                    "explorer_bridge.reconcile_pending_assignments",
                    format!(
                        "timeout pending item={} box_id={}",
                        assignment.item_path, assignment.box_id
                    ),
                );
                resolved_item_keys.push(item_key.clone());
            }
        }

        for item_key in resolved_item_keys {
            self.pending_box_assignments.remove(&item_key);
            self.observed_containers.remove(&item_key);
        }
    }

    /// 当前生效的 Box 归属由前端持久化映射和 native 临时映射共同决定，解决事件回灌前的短暂竞态。
    fn effective_assigned_box_by_item(
        &self,
        state: &NativeDesktopState,
    ) -> HashMap<String, String> {
        let mut assigned_box_by_item = Self::persisted_assigned_box_by_item(state);
        for (item_key, assignment) in &self.pending_box_assignments {
            assigned_box_by_item
                .entry(item_key.clone())
                .or_insert_with(|| assignment.box_id.clone());
        }
        assigned_box_by_item
    }

    /// 前端持久化映射是最终事实来源，native pending 只在拖放事件和前端回灌之间短暂生效。
    fn persisted_assigned_box_by_item(state: &NativeDesktopState) -> HashMap<String, String> {
        state
            .box_items
            .iter()
            .map(|item| (normalize_item_key(&item.item_path), item.box_id.clone()))
            .collect::<HashMap<_, _>>()
    }

    /// 当前端删除 Box 映射时，把此前停放到屏幕外的 Explorer 图标恢复到停放前的可见位置。
    fn restore_unassigned_parked_items(
        &mut self,
        list: HWND,
        assigned_box_by_item: &HashMap<String, String>,
    ) {
        let restore_points = self
            .parked_restore_points
            .iter()
            .filter(|(item_key, _)| !assigned_box_by_item.contains_key(*item_key))
            .map(|(item_key, restore_point)| (item_key.clone(), restore_point.clone()))
            .collect::<Vec<_>>();

        for (item_key, restore_point) in restore_points {
            log_native_desktop(
                "explorer_bridge.restore_unassigned_parked_items",
                format!(
                    "restore item_key={} to=({}, {}) index={}",
                    item_key, restore_point.screen_x, restore_point.screen_y, restore_point.index
                ),
            );
            if let Err(error) = self.set_item_position_from_screen(
                list,
                restore_point.index,
                restore_point.screen_x,
                restore_point.screen_y,
            ) {
                log_native_desktop(
                    "explorer_bridge.restore_unassigned_parked_items",
                    format!("restore failed item_key={item_key}: {error}"),
                );
                continue;
            }
            self.parked_restore_points.remove(&item_key);
            self.observed_containers.insert(item_key, None);
        }
    }

    /// 原生接管关闭或 Explorer 桥重连前，尽力把屏幕外停放的图标放回可见区域。
    ///
    /// 该方法必须在释放 Explorer 进程缓冲区之前执行，否则 `LVM_SETITEMPOSITION32` 所需的远程
    /// `POINT` 内存已经不可用。Explorer 重启等场景可能导致恢复失败，此时只能记录日志并让系统自行重建。
    fn restore_all_parked_items(&mut self) {
        if self.parked_restore_points.is_empty() {
            return;
        }
        let Some(list) = self.desktop_list.filter(|hwnd| is_live_window(*hwnd)) else {
            log_native_desktop(
                "explorer_bridge.restore_all_parked_items",
                format!(
                    "skip: desktop list unavailable count={}",
                    self.parked_restore_points.len()
                ),
            );
            return;
        };
        let restore_points = self
            .parked_restore_points
            .iter()
            .map(|(item_key, restore_point)| (item_key.clone(), restore_point.clone()))
            .collect::<Vec<_>>();

        for (item_key, restore_point) in restore_points {
            log_native_desktop(
                "explorer_bridge.restore_all_parked_items",
                format!(
                    "restore item_key={} to=({}, {}) index={}",
                    item_key, restore_point.screen_x, restore_point.screen_y, restore_point.index
                ),
            );
            if let Err(error) = self.set_item_position_from_screen(
                list,
                restore_point.index,
                restore_point.screen_x,
                restore_point.screen_y,
            ) {
                log_native_desktop(
                    "explorer_bridge.restore_all_parked_items",
                    format!("restore failed item_key={item_key}: {error}"),
                );
                continue;
            }
            self.parked_restore_points.remove(&item_key);
        }
    }
}

impl Drop for ExplorerIconBridge {
    fn drop(&mut self) {
        self.reset();
    }
}

struct ExplorerDesktopItem {
    index: i32,
    item_path: String,
    screen_x: i32,
    screen_y: i32,
}

/// drop-to-box 事件发出后、前端状态回灌前的临时归属，避免 Explorer 坐标回弹被误判成拖回桌面。
struct PendingBoxAssignment {
    box_id: String,
    item_path: String,
    created_at: Instant,
}

/// Explorer 原生图标停放到不可见区前的位置，关闭接管或移出 Box 时用于恢复可见坐标。
#[derive(Clone)]
struct ParkedIconRestorePoint {
    index: i32,
    screen_x: i32,
    screen_y: i32,
}
