mod application;
pub mod domain;
mod infrastructure;
mod interfaces;

/// 启动 Dasktop 的 Tauri 运行时，只注册当前版本真实使用的命令和插件
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_sql::Builder::default().build())
        .setup(|app| {
            if let Err(error) =
                application::native_desktop_service::set_native_desktop_icons_hidden(false)
            {
                eprintln!("failed to show native desktop icons on startup: {error}");
            }
            interfaces::tray::setup_app_tray(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            interfaces::commands::desktop_item_commands::get_desktop_items_by_paths,
            interfaces::commands::desktop_item_commands::get_desktop_snapshot,
            interfaces::commands::desktop_item_commands::open_desktop_item,
            interfaces::commands::desktop_item_commands::show_native_item_context_menu,
            interfaces::commands::native_desktop_commands::refresh_native_desktop_layer,
            interfaces::commands::native_desktop_commands::set_desktop_takeover_enabled,
            interfaces::commands::native_desktop_commands::set_native_desktop_icons_hidden,
            interfaces::commands::native_desktop_commands::sync_native_desktop_state,
            interfaces::commands::system_tray_commands::is_autostart_enabled,
            interfaces::commands::system_tray_commands::set_autostart_enabled,
            interfaces::commands::system_tray_commands::set_tray_desktop_takeover_checked,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build dasktop");

    app.run(|_app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            application::native_desktop_service::shutdown_desktop_takeover();
            if let Err(error) =
                application::native_desktop_service::set_native_desktop_icons_hidden(false)
            {
                eprintln!("failed to show native desktop icons on exit: {error}");
            }
        }
    });
}
