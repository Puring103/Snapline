# Snapline V1 设计文档

## 概述

`Snapline` 是一款快速、跨平台的个人便签应用，用来在想法出现的瞬间把它记录下来。

V1 聚焦四个特征：

- 启动快
- 记录快
- 本地优先且可靠
- 可选的多设备云同步

这个产品会刻意保持克制。它是一个个人 Markdown 便签工具，不是协作文档平台。

## 命名

- 产品名：`Snapline`
- 定位：在念头消失之前，迅速把它落成一行文字
- 气质：技术感、轻量、安静、快速

这个名字由两层含义组成：

- `snap`：瞬时捕捉、响应快
- `line`：一行文字、思路痕迹、简洁记录

## V1 范围

V1 支持：

- `Windows`、`macOS`、`Linux`
- Markdown 便签编辑
- 本地 SQLite 存储
- 全文搜索
- 软删除与恢复
- 账号登录后的云同步
- 离线优先使用
- 简单冲突处理

V1 暂不支持：

- `iOS`
- 团队协作
- 端到端加密
- 附件
- 所见即所得富文本编辑
- 实时同步

## 产品目标

Snapline 的核心体验应该是“想到就能记”。用户打开应用后，不应该等待网络或复杂初始化，应该立刻进入可输入状态。

主要目标：

- 让记录动作几乎无等待
- 保证本地数据可靠
- 让同步在后台安静进行
- 让整体技术方案足够小，能尽快交付

## 技术方向

V1 采用以 Rust 为核心的桌面应用架构，并配合 Tauri 承载 Web 编辑器界面。

- 桌面壳：`Tauri`
- UI：`React + TypeScript`
- 编辑器：`Tiptap`
- 客户端语言：`Rust`
- 本地数据库：`SQLite`
- 后台异步任务：`tokio`
- HTTP 客户端：`reqwest`
- 序列化：`serde`
- ID 生成：`uuid`
- 服务端：`Axum + PostgreSQL`

选择这条路线的原因是它符合当前优先级：

- 功能范围简单
- 强调极快启动
- 第一阶段以桌面端为主
- 当前没有立即支持 iOS 的需求
- V1 需要可编辑的渲染态 Markdown，而不是原始 Markdown 文本框
- V1 需要图片粘贴、撤销重做、列表、标题、粗体等成熟编辑行为

## 系统架构

应用建议实现为一个 Rust workspace，并拆分成职责清晰的多个 crate。

```text
snapline/
  Cargo.toml
  apps/
    desktop-tauri/
      src/
      src-tauri/
  crates/
    app-core/
    domain/
    storage/
    sync-client/
    platform/
    sync-server/
```

职责划分：

- `apps/desktop-tauri/src`：React 界面、Tiptap 编辑器、用户交互事件
- `apps/desktop-tauri/src-tauri`：Tauri 命令、窗口生命周期、前后端 IPC
- `app-core`：应用用例与业务编排
- `domain`：核心模型和业务规则
- `storage`：SQLite 访问和持久化逻辑
- `sync-client`：客户端推送/拉取同步逻辑
- `platform`：单实例、路径、系统集成等平台相关能力
- `sync-server`：最小同步后端

## 启动策略

由于“快速启动”是顶层要求，启动过程应该围绕“本地先可用”来设计，而不是等待所有模块初始化完成。

启动流程：

1. 加载配置和数据路径
2. 打开 SQLite
3. 创建 Tauri 主窗口并加载前端 bundle
4. 读取最近便签和当前便签
5. 初始化 Tiptap 编辑器并进入可编辑状态
6. 在后台初始化搜索维护
7. 在后台初始化同步任务

以下操作不应阻塞启动：

- 网络访问
- 登录状态刷新
- 全量同步
- 搜索重建
- 远端配置加载

面向用户的目标不是“所有模块已完成初始化”，而是“打开后立刻可以开始输入”。

M1 的性能目标：

- 冷启动到可输入：目标 `< 1.5s`，理想 `< 800ms`
- 输入延迟：不可感知，编辑器更新不等待 SQLite
- 自动保存：默认 `600ms` debounce，保存任务在后台执行
- 图片粘贴：先在编辑器中立即显示，再异步落盘并替换为本地资源引用
- 前端 bundle：只包含编辑器和必要 UI，避免重型组件库

## 本地数据模型

V1 使用四张核心表。

### `notes`

保存便签当前的本地状态。

建议字段：

- `id`
- `title`
- `content_md`
- `created_at`
- `updated_at`
- `deleted_at`
- `server_version`
- `last_modified_by_device`
- `is_conflict_copy`
- `source_note_id`

M1 可以先只实现 `notes` 表：

```sql
CREATE TABLE notes (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL DEFAULT '',
  content_md TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
);

CREATE INDEX idx_notes_deleted_updated
ON notes (deleted_at, updated_at DESC);
```

其中 `content_md` 是持久化格式。编辑器内部可以使用 Tiptap/ProseMirror 文档模型，但保存时必须序列化为 Markdown。

### `change_queue`

保存本地尚未上传的变更。

建议字段：

- `id`
- `note_id`
- `op_type`
- `base_version`
- `queued_at`
- `retry_count`
- `last_error`

### `sync_state`

保存每个账号在当前设备上的同步状态。

建议字段：

- `account_id`
- `device_id`
- `server_cursor`
- `last_sync_at`
- `last_success_at`

### `notes_fts`

使用 SQLite `FTS5` 建立标题和正文的全文搜索索引。

## 编辑器模型

V1 的编辑器不是原始 Markdown 源码编辑器，也不是左右分栏预览界面。

目标体验：

- 用户直接编辑渲染后的内容
- 标题、段落、列表、粗体、链接、图片在编辑区中以接近最终形态显示
- 底层仍保存 Markdown，便于同步、导出、搜索和长期维护
- 不提供单独的预览面板

M1 建议使用 `Tiptap` 作为编辑器内核。Tiptap 可以在 WebView 中处理复杂光标行为、撤销重做、粘贴、快捷键、列表缩进和图片节点。Rust 侧不实现富文本编辑器，只提供本地存储、文件系统和应用用例。

编辑保存流程：

1. 启动时 Rust 读取 `content_md`
2. 前端把 Markdown 解析为 Tiptap 文档
3. 用户在渲染态编辑器中修改内容
4. 前端 debounce 后把文档序列化为 Markdown
5. Tauri command 调用 `app-core` 保存 Markdown 到 SQLite
6. 保存成功后 UI 显示 `Saved`

## 图片粘贴

M1 支持从剪贴板粘贴图片。

图片保存位置：

```text
data/
  snapline.db
  assets/
    notes/
      <note_id>/
        <image_id>.png
```

粘贴流程：

1. 编辑器拦截 paste 事件
2. 如果剪贴板包含图片，前端先创建临时预览并插入图片节点
3. 前端通过 Tauri command 把图片字节交给 Rust
4. Rust 生成 `image_id`，写入当前 note 的 assets 目录
5. Rust 返回本地资源引用
6. 前端把临时图片节点替换为正式引用
7. 自动保存时 Markdown 中写入 `![](assets/notes/<note_id>/<image_id>.png)`

M1 不做附件管理界面，也不做跨 note 图片去重。软删除便签时先保留图片文件，后续可以增加清理任务。

## 同步模型

Snapline 采用离线优先的同步模型。

当用户编辑便签时：

1. 先写入本地数据库
2. 再把这次改动加入本地同步队列
3. 立刻向界面返回保存成功
4. 在后台稍后上传这些改动

这样可以保证记录动作足够快，也不会让记笔记依赖网络状态。

## 同步 API 形态

同步系统第一版只需要三个接口：

- `POST /sync/push`
- `GET /sync/pull?cursor=<n>`
- `GET /sync/snapshot`

### `push`

上传本地排队中的改动。

### `pull`

拉取某个游标之后发生的所有远端变更。

### `snapshot`

用于新设备首次初始化，或者本地状态重建。

## 冲突处理

V1 不使用 CRDT。

每次更新都带上一个 `base_version`。只有当客户端上传时的 `base_version` 与服务端当前版本一致，服务端才接受这次更新。

如果版本不一致：

- 保留服务端版本作为主记录
- 把本地未同步成功的编辑保存为一份冲突副本
- 在标题和界面中清楚标记它是冲突副本

这套方案简单、可预期，也能避免静默覆盖用户内容。

## 服务端模型

服务端只需要覆盖鉴权和增量同步这两个核心能力。

核心表：

- `notes`
- `change_log`
- `devices`

其中 `change_log` 作为增量事件流，供客户端按游标进行拉取同步。

## 用户体验原则

Snapline 应该给人稳定、安静、可信赖的感觉。

体验上优先保证：

- 立即记录
- 低操作摩擦
- 正常使用时尽量不暴露同步复杂性
- 冲突出现时给出可理解的恢复路径

界面应保持简洁，不把 V1 做成厚重的笔记本或工作台产品。

编辑体验上，用户不应主要面对 Markdown 标记文本。Markdown 是存储和互操作格式，而不是 M1 的主要编辑界面。

## 开发里程碑

### M1：本地 MVP

- Tauri 桌面壳
- React + Tiptap 渲染态 Markdown 编辑器
- 新建、编辑、删除便签
- SQLite 持久化
- 自动保存
- 图片粘贴到本地 assets 目录

### M2：搜索与便签流转

- FTS 搜索
- 最近便签
- 回收站恢复

### M3：同步模拟

- 本地同步抽象
- mock push/pull
- 冲突路径测试

### M4：同步后端

- Axum 服务端
- PostgreSQL 表结构
- push、pull、snapshot 接口
- 账号鉴权

### M5：多设备验证

- 双设备测试流程
- 同步重试
- 删除传播
- 冲突副本行为验证

### M6：启动优化

- 启动耗时测量
- 子系统延迟初始化
- 查询与索引调优

## 风险与约束

- Tauri + Web 编辑器的冷启动通常会比纯 Rust/Slint UI 更重，因此 M1 必须尽早加入启动耗时测量。
- Web 编辑器显著降低富文本 Markdown 编辑、图片粘贴和撤销重做的实现风险。
- Tiptap Markdown 能力需要在 M1 做 round-trip 测试，因为 Markdown 解析和序列化如果丢失节点，会直接影响本地存储可靠性。
- 如果未来把 iOS 重新列为高优先级，需要重新评估桌面技术路线与移动端复用策略。
- 为了保证交付速度，V1 的同步逻辑应保持简单。
- 如果未来把“全局快捷键快速记录”变成硬需求，应用可能需要一个可选的登录时辅助进程。

## 决策摘要

当前 V1 项目方向如下：

- 名称：`Snapline`
- 平台：桌面优先
- UI：`Tauri + React + Tiptap`
- 核心实现：`Rust`
- 存储：`SQLite`
- 同步模型：离线优先、push/pull、基于版本的冲突处理

这条路线让第一版可以把重点放在速度、简洁和可靠性上，先做出一个真正顺手的个人灵感捕捉工具。
