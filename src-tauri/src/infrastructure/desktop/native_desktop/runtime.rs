use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

use tauri::AppHandle;

use crate::domain::desktop::NativeDesktopState;
use crate::infrastructure::desktop::{log_native_desktop, set_native_desktop_icons_hidden};

/// 原生桌面接管控制器的全局句柄，Tauri 命令和退出清理都通过它调度 Win32 线程。
static NATIVE_DESKTOP: OnceLock<Mutex<NativeDesktopRuntime>> = OnceLock::new();

/// 原生控制器内部命令必须跨线程传递，因此只使用可拥有的数据结构。
#[derive(Clone)]
pub(crate) enum NativeDesktopCommand {
    SetEnabled(bool),
    SyncState(NativeDesktopState),
    Refresh,
    Shutdown,
}

/// 控制器线程句柄和发送端，负责把 Tauri 命令转发给 Win32 message loop。
struct NativeDesktopRuntime {
    command_sender: Option<Sender<NativeDesktopCommand>>,
}

impl NativeDesktopRuntime {
    fn new() -> Self {
        Self {
            command_sender: None,
        }
    }

    fn send(&mut self, app: &AppHandle, command: NativeDesktopCommand) -> Result<(), String> {
        log_native_desktop("runtime.send", describe_native_desktop_command(&command));
        if self.command_sender.is_none() {
            log_native_desktop("runtime.send", "spawning native desktop thread");
            self.command_sender = Some(spawn_native_desktop_thread(app.clone())?);
        }

        let command = if let Some(sender) = &self.command_sender {
            match sender.send(command) {
                Ok(()) => {
                    log_native_desktop(
                        "runtime.send",
                        "command delivered to native desktop thread",
                    );
                    return Ok(());
                }
                Err(error) => error.0,
            }
        } else {
            command
        };

        log_native_desktop(
            "runtime.send",
            "command channel was closed, respawning thread",
        );
        self.command_sender = Some(spawn_native_desktop_thread(app.clone())?);
        self.command_sender
            .as_ref()
            .ok_or_else(|| "原生桌面控制器未初始化".to_string())?
            .send(command)
            .map_err(|error| format!("无法唤醒原生桌面控制器：{error}"))
    }
}

/// 开启或关闭桌面接管；Explorer 原生图标始终保持可见，确保桌面基础交互由系统负责。
pub fn set_desktop_takeover_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    log_native_desktop(
        "api.set_desktop_takeover_enabled",
        format!("enabled={enabled}"),
    );
    runtime()?
        .lock()
        .map_err(|_| "原生桌面控制器状态已损坏".to_string())?
        .send(app, NativeDesktopCommand::SetEnabled(enabled))
}

/// 同步完整原生桌面状态；前端仍是 SQLite 持久化源，Rust 只保留运行时副本。
pub fn sync_native_desktop_state(app: &AppHandle, state: NativeDesktopState) -> Result<(), String> {
    log_native_desktop(
        "api.sync_native_desktop_state",
        format!(
            "items={} boxes={} box_items={} positions={}",
            state.items.len(),
            state.boxes.len(),
            state.box_items.len(),
            state.icon_positions.len()
        ),
    );
    runtime()?
        .lock()
        .map_err(|_| "原生桌面控制器状态已损坏".to_string())?
        .send(app, NativeDesktopCommand::SyncState(state))
}

/// 主动刷新原生桌面层，用于 Explorer 重启、设置变化或前端重新扫描后的显式重绘。
pub fn refresh_native_desktop_layer(app: &AppHandle) -> Result<(), String> {
    log_native_desktop("api.refresh_native_desktop_layer", "refresh requested");
    runtime()?
        .lock()
        .map_err(|_| "原生桌面控制器状态已损坏".to_string())?
        .send(app, NativeDesktopCommand::Refresh)
}

/// 应用退出时关闭接管线程并恢复 Explorer 图标层，这是异常路径之外的正常清理入口。
pub fn shutdown_desktop_takeover() {
    log_native_desktop("api.shutdown_desktop_takeover", "shutdown requested");
    if let Some(runtime) = NATIVE_DESKTOP.get() {
        if let Ok(mut runtime) = runtime.lock() {
            if let Some(sender) = runtime.command_sender.take() {
                let _ = sender.send(NativeDesktopCommand::Shutdown);
            }
        }
    }
    let _ = set_native_desktop_icons_hidden(false);
}

fn runtime() -> Result<&'static Mutex<NativeDesktopRuntime>, String> {
    Ok(NATIVE_DESKTOP.get_or_init(|| Mutex::new(NativeDesktopRuntime::new())))
}

/// 描述跨线程命令，用于记录原生接管问题时快速判断状态同步是否到达控制器。
pub(crate) fn describe_native_desktop_command(command: &NativeDesktopCommand) -> String {
    match command {
        NativeDesktopCommand::SetEnabled(enabled) => format!("SetEnabled enabled={enabled}"),
        NativeDesktopCommand::SyncState(state) => format!(
            "SyncState items={} boxes={} box_items={} positions={}",
            state.items.len(),
            state.boxes.len(),
            state.box_items.len(),
            state.icon_positions.len()
        ),
        NativeDesktopCommand::Refresh => "Refresh".to_string(),
        NativeDesktopCommand::Shutdown => "Shutdown".to_string(),
    }
}

fn spawn_native_desktop_thread(app: AppHandle) -> Result<Sender<NativeDesktopCommand>, String> {
    let (sender, receiver) = mpsc::channel();
    log_native_desktop("runtime.spawn", "creating dasktop-native-desktop thread");
    thread::Builder::new()
        .name("dasktop-native-desktop".to_string())
        .spawn(move || run_native_desktop_controller(app, receiver))
        .map_err(|error| format!("无法启动原生桌面线程：{error}"))?;

    log_native_desktop("runtime.spawn", "native desktop thread spawned");
    Ok(sender)
}

#[cfg(target_os = "windows")]
fn run_native_desktop_controller(app: AppHandle, receiver: Receiver<NativeDesktopCommand>) {
    super::windows::run(app, receiver);
}

#[cfg(not(target_os = "windows"))]
fn run_native_desktop_controller(_app: AppHandle, receiver: Receiver<NativeDesktopCommand>) {
    for command in receiver {
        if matches!(command, NativeDesktopCommand::Shutdown) {
            break;
        }
    }
}
