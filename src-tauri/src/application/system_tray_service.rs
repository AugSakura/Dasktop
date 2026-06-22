//! 托盘应用服务只处理系统自启等非 UI 状态，菜单项勾选仍由 interfaces 层同步。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// 读取系统开机自启状态，供托盘初始化和设置页命令共享。
pub fn resolve_autostart_enabled(app: &AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| format!("无法读取开机自启状态：{error}"))
}

/// 写入系统开机自启状态；UI 状态同步由调用方负责，避免服务层依赖托盘控件。
pub fn set_autostart_enabled(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch
            .enable()
            .map_err(|error| format!("无法开启开机自启：{error}"))?;
    } else {
        autolaunch
            .disable()
            .map_err(|error| format!("无法关闭开机自启：{error}"))?;
    }

    Ok(enabled)
}
