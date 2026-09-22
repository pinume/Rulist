# Rulist

A lightweight, high-performance file listing tool written in Rust (Rulist).

## 部署与安全要求

> [!IMPORTANT]
> **生产环境必须通过 HTTPS 或 HTTPS 反向代理访问 Rulist。**
> Rulist 遵循现代 Web 安全标准，密码在网络传输中依赖强加密传输层（TLS/HTTPS）提供保密性与防篡改保证，后端使用 Argon2id 进行安全哈希存储。请勿在未启用 HTTPS 的生产公网环境中明文传输凭据。
