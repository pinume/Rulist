# <img src="web/public/rulist.svg" width="32" alt="" /> Rulist

A lightweight, high-performance file listing tool written in Rust (Rulist).

## 架构

- `src/`：Rust 后端与交互命令行。
- `src/server/`：HTTP 路由、认证、文件与用户接口。
- `src/driver/`：存储挂载与本地文件操作。
- `migrations/`：SQLx SQLite schema 与一次性数据迁移。
- `web/`：SolidJS + Vite 前端。
- `public/dist/`：构建后嵌入 Rust 二进制的前端静态资源。
- `scripts/smoke_e2e.py`：通过真实 HTTP 入口验证登录、改密、上传、列表、预览和下载流程。

## 开发环境

- **Rust**: 1.85+
- **Node.js**: 24（推荐，最低 `>=22.13`）
- **Package Manager**: pnpm 12.5.1（支持 `corepack enable`）

后端交互控制台：

```bash
cargo run --locked -- interactive
```

启动 HTTP 服务：

```bash
cargo run --locked -- server
```

前端开发：

```bash
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm dev
```

## 构建

先生成前端静态资源，再构建 Rust：

```bash
./build-frontend.sh
cargo build --release --locked
```

## 测试

Rust 测试：

```bash
cargo test --locked
```

前端关键纯逻辑测试与类型检查：

```bash
cd web
corepack pnpm test
corepack pnpm lint
```

完整 HTTP 冒烟测试：

```bash
cargo build --locked
python3 scripts/smoke_e2e.py
```

## 部署与安全要求

> [!IMPORTANT]
> **生产环境必须通过 HTTPS 或 HTTPS 反向代理访问 Rulist。**
> Rulist 遵循现代 Web 安全标准，密码在网络传输中依赖强加密传输层（TLS/HTTPS）提供保密性与防篡改保证，后端使用 Argon2id 进行安全哈希存储。请勿在未启用 HTTPS 的生产公网环境中明文传输凭据。
