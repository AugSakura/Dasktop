use std::collections::HashMap;
use std::path::Path;

use crate::domain::desktop::DesktopItem;

/// 构建 Explorer 桌面显示名到 Dasktop 项目路径的映射，重复显示名会被标记为不可解析。
pub(super) fn build_display_name_map(items: &[DesktopItem]) -> HashMap<String, Option<String>> {
    let mut buckets: HashMap<String, Vec<String>> = HashMap::new();
    for item in items {
        for candidate in display_name_candidates(item) {
            buckets
                .entry(candidate)
                .or_default()
                .push(item.path.clone());
        }
    }

    buckets
        .into_iter()
        .map(|(name, paths)| {
            let mut unique = paths;
            unique.sort();
            unique.dedup();
            let value = if unique.len() == 1 {
                unique.into_iter().next()
            } else {
                None
            };
            (name, value)
        })
        .collect()
}

/// 按 Explorer 显示名解析稳定项目路径，解析失败时保持系统图标不被 Dasktop 干预。
pub(super) fn resolve_item_path_by_display_name(
    display_name_map: &HashMap<String, Option<String>>,
    display_name: &str,
) -> Option<String> {
    display_name_map
        .get(&normalize_display_name(display_name))
        .and_then(|value| value.clone())
}

fn display_name_candidates(item: &DesktopItem) -> Vec<String> {
    let mut candidates = vec![normalize_display_name(&item.name)];
    if !item.path.starts_with("shell::") {
        let path = Path::new(&item.path);
        if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
            candidates.push(normalize_display_name(stem));
        }
        if item.name.to_lowercase().ends_with(".lnk") {
            candidates.push(normalize_display_name(
                item.name.trim_end_matches(".lnk").trim_end_matches(".LNK"),
            ));
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn normalize_display_name(value: &str) -> String {
    value.trim().to_lowercase()
}
