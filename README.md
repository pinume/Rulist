# Rulist

A lightweight, high-performance file listing tool written in Rust (Rulist).

## 开发与构建环境

- **Rust**: 1.85+
- **Node.js**: 24（推荐，最低 `>=22.13`）
- **Package Manager**: pnpm 11.10.0（支持 `corepack enable`）

## 部署与安全要求

> [!IMPORTANT]
> **生产环境必须通过 HTTPS 或 HTTPS 反向代理访问 Rulist。**
> Rulist 遵循现代 Web 安全标准，密码在网络传输中依赖强加密传输层（TLS/HTTPS）提供保密性与防篡改保证，后端使用 Argon2id 进行安全哈希存储。请勿在未启用 HTTPS 的生产公网环境中明文传输凭据。
