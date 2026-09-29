# Rulist 重构进度

以当前分支提交历史和工作区为准。未提交的改动不视为已完成。

## 已完成

- [x] 清理目录密码流程：移除前端密码提示、`NeedPassword` 与相关重试逻辑。提交：`9d399a8`。
- [x] 清理过期文件响应字段：移除 `write_content_bypass`、`related`、`created`。提交：`af3c64f`。
- [x] 将应用配置迁入 `config.json`：删除数据库设置表与读取路径，签名密钥和 UI 设置改由配置提供。提交：`f760d5f`。
- [x] 前置清理已提交：删除废弃文件端点及 schema 元数据、清理无效应用设置，并将存储数据模型收敛为本地目录。提交：`f9fff7e`、`4b8476a`、`e761724`。
- [x] **阶段 3：用户直接绑定本地目录。** 用户根目录、文件访问、签名链接和前端路径语义已通过根代理审查与 E2E。提交：`e92123b`。
- [x] **阶段 4：删除 Storage 抽象。** 删除 `x_storages`、`StorageManager`、`Storage` 模型、"查看挂载" CLI 入口；将 `driver/` 改名为 `filesystem/`、`LocalDriver` 改为 `LocalFs`。提交：`2cb9893`。

## 当前阶段

- [ ] **阶段 5：整理 Rust 模块。** 当前下一阶段。

## 待办

- [ ] **阶段 5：整理 Rust 模块。** 引入 `app.rs`，拆分 `db/`，将类型移至相邻模块，删除 `model.rs`，将 `server/fs.rs` 改为 `server/files.rs`。
- [ ] **阶段 6：统一用户业务并重构 CLI。** HTTP 与交互式 CLI 共用用户操作和密码校验；CLI 不自行执行 SQL。
- [ ] **阶段 7：收敛前端模型。** 统一文件对象命名；确认并移除无实际用途的 `provider`、`readme`、`header` 字段。
- [ ] **阶段 8：最终验收和文档。** 更新 README，完成全新环境启动、数据库结构、残留搜索、Rust/前端构建及 E2E 验证；确认工作区干净。

### CLI 目标（阶段 6）

顶层菜单收敛为“用户列表、添加用户、管理用户、服务信息、退出”。管理用户时先选择一次用户，再进入该用户的密码、目录、权限、2FA、启用/禁用和删除操作；移除“查看挂载”。密码修改与重置合并，2FA 绑定与解绑合并。目录统一保存为用户的 `local_path`，管理员也可修改目录，但不可禁用。服务信息只读，不提供第二套配置编辑入口。保留单个 `src/interactive.rs`，不新增 CLI 框架或子命令。

## 最终验证清单

- [ ] Rust：`cargo fmt --check`、`cargo check --locked`、`cargo test --locked`、`cargo build --release --locked`。
- [ ] 前端：`pnpm lint`、`pnpm test`、`pnpm build`。
- [ ] 全新数据目录启动；确认 SQLite 仅保留用户及认证安全状态所需表，且用户记录包含 `local_path`。
- [ ] E2E：管理员和普通用户各自只能访问其本地根目录；验证路径越界防护、签名下载、目录变更后旧签名失效及用户管理流程。
- [ ] 残留搜索确认旧目录密码、Storage、`/.users/`、数据库设置系统及过期前后端字段已清理；Hope UI fork 依赖按计划保留。
- [ ] 最终 `git status` 干净；各阶段保持独立提交。
