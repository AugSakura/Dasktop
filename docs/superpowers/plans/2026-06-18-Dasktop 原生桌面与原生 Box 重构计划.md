# Dasktop 原生桌面与原生 Box 重构计划

## Summary
把当前“WebView Box + 粗糙桌面接管”切换到 Explorer-native 的 Fences 路线：不隐藏 Explorer 的 `SysListView32` 桌面图标层，不重画桌面图标，不替换 Windows 右键菜单、框选、多选、F2、Delete、拖拽等原生行为。Dasktop 只在 Explorer 图标层后方绘制原生 Box 背景，并通过 Win32 ListView 消息读取和设置 Explorer 自己的图标坐标；Box 映射仍由前端 Store 与 SQLite 持久化。

## Key Changes
- 原生接管层改为 Explorer-native overlay：负责桌面 WorkerW/Progman 挂载、Box 背景绘制、Explorer 桌面 ListView 定位、图标坐标读取与写入。
- Explorer 原生图标层始终可见；桌面背景、图标绘制、右键菜单、框选、多选、双击打开、Delete、F2、刷新和基础拖拽继续由 Windows Explorer 处理。
- Box 不再作为 Tauri/WebView 图标窗口运行；改为原生控制器里的桌面背景面板，样式按现有 Box 的标题、圆角、透明度、主题、折叠和尺寸规则对齐。
- “收纳到 Box”通过“图标坐标进入 Box 区域 + 写入 `box_items` 映射 + 后续同步将该图标停靠在 Box 网格内”实现，不移动真实文件，不隐藏 Explorer 整层。
- Rust 负责运行时 Explorer 坐标协调，前端负责持久化和设置同步：Box 映射、Box 布局、桌面/Box 内图标位置由前端保存，原生层只持有运行时副本。

## Interfaces And Data Flow
- Rust 命令：
  - `set_desktop_takeover_enabled(enabled)`
  - `sync_native_desktop_state(state)`
  - `refresh_native_desktop_layer()`
  - 通过 `dasktop://native-desktop-error` 上报接管失败。
- 运行时状态：
  - `NativeDesktopState`: 桌面项目、Box 列表、Box 映射、桌面图标位置、Box 内图标位置、显示设置、主题设置。
  - `NativeBoxState`: `id/title/x/y/width/height/collapsed/locked/titleOpacity/titlePosition/theme`。
  - `NativeIconPosition`: `itemPath + containerKind(desktop|box) + containerId + monitorId + physical x/y/orderIndex`。
- 原生层向前端发事件：
  - Explorer 图标被拖入 Box 区域：写入 `box_items` 并同步新状态。
  - 已收纳 Explorer 图标被拖回桌面区域：删除 `box_items` 映射并保存桌面落点。
  - Box 移动/缩放/折叠/标题编辑：前端保存 Box 布局后同步原生层。
  - Explorer 重启或桌面刷新：前端触发重新扫描并同步。
- 前端保留设置页与 Store；删除 WebView Box 运行时图标交互入口，设置页仍能创建、删除、配置 Box。

## Implementation Plan
- 第一阶段：替换 `desktop/takeover.rs` 自建桌面/Box ListView 原型，改为原生 Box 背景层和 Explorer 图标坐标桥；启动失败不隐藏 Explorer 图标，并写入设置页错误。
- 第二阶段：实现 Explorer 桌面 ListView 项目匹配、坐标读取、坐标写入和 Explorer 重启后重连；第一版按显示名匹配，重名项目不自动接管。
- 第三阶段：实现原生 Box 面板视觉：标题位置、主题、圆角、折叠高度、尺寸和默认布局对齐当前 Dasktop 样式；Box 面板不抢桌面鼠标事件。
- 第四阶段：实现收纳同步：桌面图标拖入 Box 区域写映射，已收纳图标拖出 Box 删除映射，已收纳图标按 Box 网格停靠；所有动作只改 Dasktop 映射和 Explorer 图标坐标，不移动真实文件。
- 第五阶段：补齐 Box 编辑能力：移动、缩放、锁定、折叠、吸附和标题编辑优先通过设置页完成，后续再评估是否用极窄命中区域支持桌面直接编辑。
- 第六阶段：清理前端旧路径：删除 WebView Box 图标运行时交互代码，保留设置页、持久化逻辑和原生同步链路。

## Test Plan
- 自动验证：
  - `cargo check --manifest-path src-tauri/Cargo.toml`
  - `pnpm build`
- 手动验证：
  - 开启接管后 Explorer 原生图标和桌面背景仍可见。
  - 桌面右键、图标右键、框选、多选、双击打开、Delete、F2、刷新仍由 Windows 原生处理。
  - 新增 Box 后桌面出现原生 Box 背景面板，设置页修改 Box 布局后同步到桌面。
  - 将 Explorer 桌面图标拖进 Box 区域后写入映射，图标停靠到 Box 网格内，真实文件路径不变。
  - 将已收纳图标拖出 Box 区域后删除映射，图标留在桌面落点，重启后保持。
  - Explorer 重启、应用退出、异常启动失败、多显示器、高 DPI 不会留下隐藏的 Explorer 图标层。

## Assumptions
- 只支持 Windows。
- 默认启用 Explorer-native 桌面模式；失败时保持 Explorer 图标可见并向设置页报告。
- 不做 Explorer 注入、跨进程子类化或物理移动文件。
- 当前项目未上线，可以删除旧字段、旧 WebView Box 运行时逻辑和旧兼容分支。
- 最终视觉要贴近当前 Dasktop Box 样式，但交互以 Windows 原生桌面手感优先。
