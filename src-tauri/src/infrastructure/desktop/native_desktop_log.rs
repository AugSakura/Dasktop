use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{process, thread};

/// 诊断日志可能同时由 Tauri 主线程和原生桌面线程写入，串行化可以避免单行日志互相穿插。
static NATIVE_DESKTOP_LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 写入原生桌面接管诊断日志，并在每条日志后立即刷新到磁盘。
///
/// 接管崩溃通常表现为进程级访问违例，标准输出和前端日志很容易来不及写完。
/// 因此这里刻意采用每次打开、写入、flush 的方式，牺牲少量性能换取崩溃前最后阶段可见。
pub fn log_native_desktop(stage: &str, detail: impl AsRef<str>) {
    let _guard = NATIVE_DESKTOP_LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .ok();
    let path = native_desktop_log_path();
    if let Some(parent) = path.parent() {
        let _ = create_dir_all(parent);
    }

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let _ = writeln!(
        file,
        "[{timestamp}] [pid={}] [thread={:?}] [{stage}] {}",
        process::id(),
        thread::current().id(),
        detail.as_ref()
    );
    let _ = file.flush();
}

/// 返回固定诊断日志位置，方便用户复现后直接读取尾部内容。
pub fn native_desktop_log_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("native-desktop-takeover.log")
}
