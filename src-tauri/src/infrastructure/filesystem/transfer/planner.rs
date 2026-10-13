//! 文件传输计划器负责把拖拽动作、目标目录和冲突策略解析成可执行计划。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::domain::desktop::{BoxConflictPolicy, BoxDropAction};
use crate::domain::filesystem::is_windows_shortcut_extension;
use crate::infrastructure::filesystem::naming;
use crate::infrastructure::filesystem::transfer::plan::{FileTransferKind, FileTransferPlan};

/// 为一批拖入项目创建执行计划；同目录移动和冲突跳过不会进入最终计划。
pub(super) fn create_transfer_plans(
    paths: &[PathBuf],
    destination: &Path,
    action: BoxDropAction,
    conflict_policy: BoxConflictPolicy,
) -> Result<Vec<FileTransferPlan>, String> {
    let mut reserved_paths = HashSet::new();
    let mut plans = Vec::new();

    for source in paths {
        if should_skip_noop_move(source, destination, action)? {
            continue;
        }
        ensure_move_target_is_not_inside_source(source, destination, action)?;

        let (kind, desired_destination) =
            resolve_desired_transfer_destination(source, destination, action)?;
        let Some(destination_path) = resolve_conflict_destination(
            source,
            &desired_destination,
            conflict_policy,
            &mut reserved_paths,
        )?
        else {
            continue;
        };

        plans.push(FileTransferPlan {
            source: source.clone(),
            replaces_existing: should_prepare_replace_backup(conflict_policy, &destination_path),
            destination: destination_path,
            kind,
        });
    }

    Ok(plans)
}

fn should_skip_noop_move(
    source: &Path,
    destination: &Path,
    action: BoxDropAction,
) -> Result<bool, String> {
    // 同目录移动没有真实文件变化，提前跳过可以避免后续把自身当成冲突目标处理。
    Ok(action == BoxDropAction::Move
        && naming::is_direct_child_of_destination(source, destination)?)
}

fn ensure_move_target_is_not_inside_source(
    source: &Path,
    destination: &Path,
    action: BoxDropAction,
) -> Result<(), String> {
    // Windows 不允许把文件夹移动到自身内部；提前拦截比等待 fs::rename 报模糊错误更清晰。
    if action == BoxDropAction::Move && naming::destination_is_inside_source(source, destination)? {
        return Err("不能把文件夹移动到自身内部".to_string());
    }

    Ok(())
}

fn resolve_desired_transfer_destination(
    source: &Path,
    destination: &Path,
    action: BoxDropAction,
) -> Result<(FileTransferKind, PathBuf), String> {
    if action == BoxDropAction::Map && !is_windows_shortcut(source) {
        return Ok((
            FileTransferKind::Shortcut,
            naming::desired_shortcut_path(source, destination),
        ));
    }

    let file_name = source
        .file_name()
        .ok_or_else(|| "无法解析待处理项目名称".to_string())?;
    let kind = match action {
        BoxDropAction::Copy | BoxDropAction::Map => FileTransferKind::Copy,
        BoxDropAction::Move => FileTransferKind::Move,
    };

    Ok((kind, destination.join(file_name)))
}

fn resolve_conflict_destination(
    source: &Path,
    desired_destination: &Path,
    conflict_policy: BoxConflictPolicy,
    reserved_paths: &mut HashSet<String>,
) -> Result<Option<PathBuf>, String> {
    if conflict_policy == BoxConflictPolicy::Replace
        && naming::paths_refer_to_same_entry(source, desired_destination)?
    {
        return Ok(None);
    }

    match conflict_policy {
        BoxConflictPolicy::Rename => {
            naming::resolve_renamed_destination(desired_destination, reserved_paths).map(Some)
        }
        BoxConflictPolicy::Skip => {
            if desired_destination.exists()
                || !naming::reserve_path(desired_destination, reserved_paths)?
            {
                return Ok(None);
            }

            Ok(Some(desired_destination.to_path_buf()))
        }
        BoxConflictPolicy::Replace => {
            // 同一批次内两个来源解析到同一路径时跳过后者，避免“替换”把前一个刚传输的文件覆盖掉。
            if !naming::reserve_path(desired_destination, reserved_paths)? {
                return Ok(None);
            }

            Ok(Some(desired_destination.to_path_buf()))
        }
    }
}

fn should_prepare_replace_backup(conflict_policy: BoxConflictPolicy, destination: &Path) -> bool {
    conflict_policy == BoxConflictPolicy::Replace && destination.exists()
}

/// 已是 Windows 快捷方式家族的来源直接复制，避免映射时套出指向快捷方式的二级快捷方式。
fn is_windows_shortcut(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_windows_shortcut_extension)
}
