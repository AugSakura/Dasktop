use crate::domain::desktop::{NativeBoxState, NativeDesktopDisplaySettings, NativeDesktopState};

/// 原生桌面线程的运行时状态模型，负责把前端状态规整成 Win32 控制器可直接消费的数据。
#[derive(Clone, Default)]
pub(crate) struct NativeDesktopModel {
    /// 接管开关只代表原生线程当前是否应该维护窗口和 Explorer 图标同步。
    pub(crate) enabled: bool,
    state: Option<NativeDesktopState>,
}

impl NativeDesktopModel {
    /// 写入前端同步来的完整状态，并在边界处规整尺寸、透明度等显示参数。
    pub(crate) fn apply_state(&mut self, state: NativeDesktopState) {
        self.state = Some(NativeDesktopState {
            items: state.items,
            boxes: state.boxes.into_iter().map(sanitize_box_state).collect(),
            box_items: state.box_items,
            icon_positions: state.icon_positions,
            display: sanitize_display_settings(state.display),
        });
    }

    /// 读取当前原生层快照；没有前端状态时原生层只维持空窗口和清理逻辑。
    pub(crate) fn state(&self) -> Option<&NativeDesktopState> {
        self.state.as_ref()
    }

    /// 标题栏拖动结束后先更新线程内状态，避免下一次局部重绘使用旧坐标。
    pub(crate) fn update_box_position(&mut self, box_id: &str, x: i32, y: i32) {
        if let Some(state) = &mut self.state {
            if let Some(box_state) = state
                .boxes
                .iter_mut()
                .find(|box_state| box_state.id == box_id)
            {
                box_state.x = x;
                box_state.y = y;
            }
        }
    }
}

fn sanitize_box_state(mut box_state: NativeBoxState) -> NativeBoxState {
    box_state.width = box_state.width.max(180);
    box_state.height = box_state.height.max(96);
    box_state.title_opacity = box_state.title_opacity.clamp(0, 100);
    box_state
}

fn sanitize_display_settings(
    display: NativeDesktopDisplaySettings,
) -> NativeDesktopDisplaySettings {
    NativeDesktopDisplaySettings {
        icon_size: display.icon_size.clamp(28, 96),
        label_text_size: display.label_text_size.clamp(9.0, 18.0),
        label_width: display.label_width.clamp(56, 180),
        icon_gap_x: display.icon_gap_x.clamp(0, 64),
        icon_gap_y: display.icon_gap_y.clamp(0, 64),
        corner_radius: display.corner_radius.clamp(0, 24),
        box_background_opacity: display.box_background_opacity.clamp(0, 100),
        show_item_labels: display.show_item_labels,
        show_shortcut_arrow: display.show_shortcut_arrow,
    }
}

/// 统一归一化桌面项目主键，避免 Windows 路径大小写和分隔符差异导致 Box 映射失效。
pub(crate) fn normalize_item_key(path: &str) -> String {
    path.trim().replace('/', "\\").to_lowercase()
}
