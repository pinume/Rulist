use crate::auth;
use crate::db::{self, DbPool};
use crate::model;
use anyhow::Result;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

struct PermItem {
    bit: i32,
    name: &'static str,
}

const PERM_ITEMS: &[PermItem] = &[
    PermItem {
        bit: 3,
        name: "新建与上传",
    },
    PermItem {
        bit: 4,
        name: "重命名",
    },
    PermItem {
        bit: 5,
        name: "移动",
    },
    PermItem {
        bit: 6,
        name: "复制",
    },
    PermItem {
        bit: 7,
        name: "删除",
    },
    PermItem {
        bit: 8,
        name: "覆盖同名文件",
    },
    PermItem {
        bit: 9,
        name: "免密登录",
    },
];

pub fn format_permissions(role: i32, perm: i32) -> String {
    if role == model::ROLE_ADMIN {
        return "完全控制 (管理员)".to_string();
    }
    let file_perm = perm & !(1 << model::PERM_ALLOW_EMPTY_PASSWORD);
    let allow_empty = (perm & (1 << model::PERM_ALLOW_EMPTY_PASSWORD)) != 0;

    let base = match file_perm {
        504 => "全部文件权限".to_string(),
        0 => "只读浏览".to_string(),
        _ => {
            let mut names = Vec::new();
            if perm & (1 << 3) != 0 {
                names.push("上传新建");
            }
            if perm & (1 << 4) != 0 {
                names.push("重命名");
            }
            if perm & (1 << 5) != 0 {
                names.push("移动");
            }
            if perm & (1 << 6) != 0 {
                names.push("复制");
            }
            if perm & (1 << 7) != 0 {
                names.push("删除");
            }
            if perm & (1 << 8) != 0 {
                names.push("覆盖");
            }
            if names.is_empty() {
                "无文件权限".to_string()
            } else {
                names.join(",")
            }
        }
    };

    if allow_empty {
        format!("{},免密", base)
    } else {
        base
    }
}

fn prompt(label: &str) -> io::Result<String> {
    print!("{}", label);
    io::stdout().flush()?;
    let mut input = String::new();
    let n = io::stdin().read_line(&mut input)?;
    if n == 0 {
        return Ok("q".to_string());
    }
    Ok(input.trim().to_string())
}

fn prompt_default(label: &str, default: &str) -> io::Result<String> {
    print!("{} [{}]: ", label, default);
    io::stdout().flush()?;
    let mut input = String::new();
    let n = io::stdin().read_line(&mut input)?;
    if n == 0 {
        return Ok(default.to_string());
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn pause() {
    print!("\n按回车键继续...");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);
}

fn resolve_storage_path(input: &str, default_home: &str) -> PathBuf {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return PathBuf::from(default_home);
    }
    if trimmed.starts_with(default_home) {
        return PathBuf::from(trimmed);
    }
    if trimmed.starts_with('~') {
        let sub = trimmed.trim_start_matches('~').trim_start_matches('/');
        return PathBuf::from(default_home).join(sub);
    }
    let p = Path::new(trimmed);
    if p.is_absolute() && p.exists() {
        return p.to_path_buf();
    }
    let sub = trimmed.trim_start_matches('/');
    PathBuf::from(default_home).join(sub)
}

fn parse_perm_input(input: &str) -> Option<i32> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("all") {
        let mut perm = 0;
        for item in PERM_ITEMS {
            perm |= 1 << item.bit;
        }
        return Some(perm);
    }
    if trimmed.eq_ignore_ascii_case("none") || trimmed == "0" {
        return Some(0);
    }

    let mut perm = 0;
    if trimmed.contains(|c: char| c.is_whitespace() || c == ',') {
        let parts = trimmed.split(|c: char| c.is_whitespace() || c == ',');
        for part in parts {
            if part.is_empty() {
                continue;
            }
            if let Ok(num) = part.parse::<usize>() {
                if num >= 1 && num <= PERM_ITEMS.len() {
                    perm |= 1 << PERM_ITEMS[num - 1].bit;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        Some(perm)
    } else {
        for ch in trimmed.chars() {
            if let Some(digit) = ch.to_digit(10) {
                let num = digit as usize;
                if num >= 1 && num <= PERM_ITEMS.len() {
                    perm |= 1 << PERM_ITEMS[num - 1].bit;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        Some(perm)
    }
}

fn select_permissions() -> io::Result<Option<i32>> {
    println!();
    println!("--------------------------------------------------");
    println!("权限列表:");
    println!("--------------------------------------------------");
    for (idx, item) in PERM_ITEMS.iter().enumerate() {
        println!("  {}. {}", idx + 1, item.name);
    }
    println!("--------------------------------------------------");

    loop {
        let input =
            prompt("请输入要开启的权限序号 (如 1 2 3 5 6，输入 all 全选，输入 0 为无权限): ")?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            println!("未输入权限序号，请选择要开启的权限序号。");
            continue;
        }
        if trimmed.eq_ignore_ascii_case("q") || trimmed == "cancel" {
            return Ok(None);
        }
        if let Some(perm) = parse_perm_input(trimmed) {
            return Ok(Some(perm));
        }
        println!(
            "输入无效，请输入 1-{} 的序号 (如 1 2 3 或 12356)，输入 all 全选，输入 0 为无权限。",
            PERM_ITEMS.len()
        );
    }
}

pub async fn run_interactive_console(pool: &DbPool, _data_dir: &Path) -> Result<()> {
    loop {
        println!();
        println!("==================================================");
        println!("                Rulist 控制台管理");
        println!("==================================================");
        println!(" 1. 查看用户列表");
        println!(" 2. 添加新用户");
        println!(" 3. 管理指定用户");
        println!(" 4. 查看存储挂载点");
        println!(" 5. 查看管理员令牌");
        println!(" 0. 退出控制台");
        println!("==================================================");

        let choice = prompt("请选择操作 [0-5]: ")?;
        match choice.as_str() {
            "1" => {
                println!();
                print_users_table(pool).await?;
                pause();
            }
            "2" => {
                action_add_user(pool).await?;
                pause();
            }
            "3" => {
                action_manage_user(pool).await?;
            }
            "4" => {
                action_list_storages(pool).await?;
            }
            "5" => {
                if let Some(token) = db::get_setting(pool, "token").await? {
                    println!("\nAdmin Token: {}", token);
                } else {
                    println!("\n未找到管理员 Token。");
                }
                pause();
            }
            "0" | "q" | "exit" => {
                println!("已退出 Rulist 交互控制台。");
                break;
            }
            _ => {
                println!("无效输入，请重新选择。");
            }
        }
    }

    Ok(())
}

async fn print_users_table(pool: &DbPool) -> Result<()> {
    let users = db::get_all_users(pool).await?;
    let storages = db::get_all_storages(pool).await?;
    println!(
        "{:<4} {:<16} {:<10} {:<10} {:<10} {:<24} {}",
        "ID", "USERNAME", "ROLE", "STATUS", "2FA", "LOCAL DIRECTORY", "PERMISSIONS"
    );
    println!("{}", "-".repeat(95));
    if users.is_empty() {
        println!("(暂无用户)");
    } else {
        for u in users {
            let role_str = if u.is_admin() { "admin" } else { "general" };
            let status_str = if u.disabled { "disabled" } else { "enabled" };
            let two_fa_str = if u.otp { "enabled" } else { "disabled" };
            let mut local_dir = db::compute_local_path(&u.base_path, &storages);
            if local_dir.is_empty() {
                local_dir = u.base_path.clone();
            }
            let perm_str = format_permissions(u.role, u.permission);
            println!(
                "{:<4} {:<16} {:<10} {:<10} {:<10} {:<24} {}",
                u.id, u.username, role_str, status_str, two_fa_str, local_dir, perm_str
            );
        }
    }
    Ok(())
}

async fn action_add_user(pool: &DbPool) -> Result<()> {
    println!();
    println!(">>> 添加新用户");
    let username = prompt("请输入用户名: ")?;
    if username.is_empty() || username == "q" {
        return Ok(());
    }
    if db::get_user_by_name(pool, &username).await?.is_some() {
        println!("错误: 用户 '{}' 已存在。", username);
        return Ok(());
    }

    let default_home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let d = prompt_default("存储目录路径", &default_home)?;
    let path = resolve_storage_path(&d, &default_home);
    if !path.exists() {
        let create_it = prompt_default(&format!("目录 {:?} 不存在，是否创建？ (y/n)", path), "y")?;
        if create_it.eq_ignore_ascii_case("y") {
            if let Err(e) = std::fs::create_dir_all(&path) {
                println!("创建目录失败: {}", e);
                return Ok(());
            }
        } else {
            println!("操作已取消。");
            return Ok(());
        }
    }
    let canon = match path.canonicalize() {
        Ok(c) => c.to_string_lossy().into_owned(),
        Err(e) => {
            println!("路径无效: {}", e);
            return Ok(());
        }
    };
    let dir_input = Some(canon);

    let perm = loop {
        if let Some(p) = select_permissions()? {
            break p;
        }
        println!("未配置权限，添加用户必须指定权限序号。");
    };

    let password = loop {
        let pwd = prompt("请输入登录密码 (若已选免密登录可直接回车留空): ")?;
        if pwd.is_empty() {
            if (perm & (1 << model::PERM_ALLOW_EMPTY_PASSWORD)) != 0 {
                break String::new();
            } else {
                println!("用户未开启「免密登录」权限，密码不能为空，请输入密码。");
                continue;
            }
        }
        let confirm = prompt("请再次输入以确认密码: ")?;
        if pwd != confirm {
            println!("错误: 两次输入的密码不一致，请重新输入。");
            continue;
        }
        break pwd;
    };

    match db::create_user_direct(
        pool,
        &username,
        &password,
        0,
        dir_input.as_deref(),
        perm,
        false,
    )
    .await
    {
        Ok(id) => {
            println!("\n用户 '{}' 创建成功！", username);
            println!("  ID: {}", id);
            println!("  角色: 普通用户 (general)");
            println!("  目录: {}", dir_input.unwrap_or_else(|| "/".to_string()));
            println!("  权限: {}", format_permissions(0, perm));
        }
        Err(e) => {
            println!("创建用户失败: {}", e);
        }
    }
    Ok(())
}

async fn action_manage_user(pool: &DbPool) -> Result<()> {
    let mut user = loop {
        println!();
        let query = prompt("请输入要管理的用户 ID 或用户名 (输入 ? 查看列表，留空返回): ")?;
        if query.is_empty() || query == "q" {
            return Ok(());
        }
        if query == "?" || query == "list" {
            println!();
            print_users_table(pool).await?;
            continue;
        }

        let user_opt = if let Ok(id) = query.parse::<i64>() {
            db::get_user_by_id(pool, id).await?
        } else {
            db::get_user_by_name(pool, &query).await?
        };

        match user_opt {
            Some(u) => break u,
            None => {
                println!("错误: 未找到用户 '{}'。", query);
            }
        }
    };

    loop {
        let storages = db::get_all_storages(pool).await?;
        let mut local_dir = db::compute_local_path(&user.base_path, &storages);
        if local_dir.is_empty() {
            local_dir = user.base_path.clone();
        }

        println!();
        println!("--------------------------------------------------");
        println!(" 用户详情: {} (ID: {})", user.username, user.id);
        println!(
            "   角色: {}",
            if user.is_admin() {
                "管理员 (admin)"
            } else {
                "普通用户 (general)"
            }
        );
        println!(
            "   状态: {} | 2FA: {}",
            if user.disabled {
                "已禁用 (disabled)"
            } else {
                "正常 (enabled)"
            },
            if user.otp { "已启用" } else { "未启用" }
        );
        println!("   本地目录: {}", local_dir);
        println!(
            "   操作权限: {}",
            format_permissions(user.role, user.permission)
        );
        println!("--------------------------------------------------");
        println!(" 1. 修改登录密码");
        if !user.is_admin() {
            println!(" 2. 设置本地存储目录");
            println!(" 3. 设置用户操作权限");
            println!(
                " 4. 切换 启用 / 禁用状态 (当前: {})",
                if user.disabled { "已禁用" } else { "正常" }
            );
        }
        println!(" 5. 重置密码为随机字符串并解绑 2FA");
        println!(" 6. 解绑 2FA (双因素认证)");
        if !user.is_admin() {
            println!(" 7. 删除此用户");
        }
        println!(" 0. 返回主控制台");
        println!("--------------------------------------------------");

        let choice = prompt("请选择操作: ")?;
        match choice.as_str() {
            "1" => {
                let password = loop {
                    let pwd = prompt("请输入新密码 (直接回车表示留空免密): ")?;
                    if pwd.is_empty() {
                        if user.is_admin() {
                            println!("错误: 管理员账户不允许设置为空密码。");
                            continue;
                        }
                        let allow = prompt_default(
                            "检测到密码为空，是否确认设置为空密码并开启免密登录权限？(y/N)",
                            "n",
                        )?;
                        if allow.eq_ignore_ascii_case("y") {
                            let new_perm =
                                user.permission | (1 << model::PERM_ALLOW_EMPTY_PASSWORD);
                            db::set_user_permission(pool, user.id, new_perm).await?;
                            user.permission = new_perm;
                            break String::new();
                        } else {
                            println!("操作已取消，请重新输入密码。");
                            continue;
                        }
                    }
                    if user.is_admin() && !auth::valid_password(&pwd) {
                        println!("错误: 管理员密码长度需在 8 到 128 位之间。");
                        continue;
                    }
                    let confirm = prompt("请再次输入确认密码: ")?;
                    if pwd != confirm {
                        println!("错误: 两次输入的密码不一致，请重新输入。");
                        continue;
                    }
                    break pwd;
                };

                db::set_user_password(pool, &user.username, &password, false).await?;
                println!("用户 '{}' 的密码已成功更新！", user.username);
                pause();
            }
            "2" if !user.is_admin() => {
                let default_home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                let dir_input = prompt_default("请输入新的本地目录路径", &default_home)?;
                let path = resolve_storage_path(&dir_input, &default_home);
                if !path.exists() {
                    let create_it =
                        prompt_default(&format!("目录 {:?} 不存在，是否创建？ (y/n)", path), "y")?;
                    if create_it.eq_ignore_ascii_case("y") {
                        if let Err(e) = std::fs::create_dir_all(&path) {
                            println!("创建目录失败: {}", e);
                            pause();
                            continue;
                        }
                    } else {
                        println!("操作已取消。");
                        pause();
                        continue;
                    }
                }
                let canon = match path.canonicalize() {
                    Ok(c) => c.to_string_lossy().into_owned(),
                    Err(e) => {
                        println!("路径无效: {}", e);
                        pause();
                        continue;
                    }
                };

                db::set_user_dir(pool, user.id, &canon).await?;
                println!("用户 '{}' 的存储目录已成功更新为: {}", user.username, canon);
                pause();
            }
            "3" if !user.is_admin() => {
                println!();
                println!(
                    "用户 '{}' 当前权限: {}",
                    user.username,
                    format_permissions(user.role, user.permission)
                );
                if let Some(new_perm) = select_permissions()? {
                    db::set_user_permission(pool, user.id, new_perm).await?;
                    user.permission = new_perm;
                    println!(
                        "用户 '{}' 的权限已更新为: {}",
                        user.username,
                        format_permissions(user.role, new_perm)
                    );
                } else {
                    println!("操作已取消，权限未作更改。");
                }
                pause();
            }
            "4" if !user.is_admin() => {
                let new_disabled = !user.disabled;
                db::set_user_disabled(pool, user.id, new_disabled).await?;
                user.disabled = new_disabled;
                println!(
                    "用户 '{}' 已成功切换为: {}",
                    user.username,
                    if new_disabled { "禁用" } else { "启用" }
                );
                pause();
            }
            "5" => {
                let confirm = prompt_default(
                    &format!(
                        "确定要重置用户 '{}' 的密码并解绑 2FA 吗？(y/N)",
                        user.username
                    ),
                    "n",
                )?;
                if confirm.eq_ignore_ascii_case("y") {
                    let new_pwd = auth::rand_string(16);
                    db::set_user_password(pool, &user.username, &new_pwd, true).await?;
                    user.otp = false;
                    println!("密码已重置，且 2FA 已解绑！");
                    println!("用户名: {}", user.username);
                    println!("新密码: {}", new_pwd);
                } else {
                    println!("操作已取消。");
                }
                pause();
            }
            "6" => {
                db::cancel_user_2fa(pool, user.id).await?;
                user.otp = false;
                println!(
                    "用户 '{}' 的双因素认证 (2FA) 已成功解除绑定。",
                    user.username
                );
                pause();
            }
            "7" if !user.is_admin() => {
                let confirm = prompt_default(
                    &format!(
                        "确定要彻底删除用户 '{}' 及其存储挂载点吗？(y/N)",
                        user.username
                    ),
                    "n",
                )?;
                if confirm.eq_ignore_ascii_case("y") {
                    db::delete_user(pool, user.id).await?;
                    println!("用户 '{}' 已成功删除。", user.username);
                    pause();
                    break;
                } else {
                    println!("操作已取消。");
                    pause();
                }
            }
            "0" | "q" | "back" => break,
            _ => {
                println!("无效输入，请重试。");
            }
        }
    }
    Ok(())
}

async fn action_list_storages(pool: &DbPool) -> Result<()> {
    let storages = db::get_all_storages(pool).await?;
    if storages.is_empty() {
        println!("\n暂无存储挂载点。");
    } else {
        println!();
        println!(
            "{:<4} {:<18} {:<10} {:<10} {}",
            "ID", "MOUNT PATH", "DRIVER", "STATUS", "LOCAL PATH / DETAILS"
        );
        println!("{}", "-".repeat(80));
        for s in storages {
            let status_str = if s.disabled {
                "disabled"
            } else {
                s.status.as_deref().unwrap_or("active")
            };
            let mut detail = String::new();
            if let Some(ref add_str) = s.addition
                && let Ok(val) = serde_json::from_str::<serde_json::Value>(add_str)
                && let Some(root) = val
                    .get("root_folder_path")
                    .or_else(|| val.get("root_folder"))
                    .and_then(|v| v.as_str())
            {
                detail = root.to_string();
            } else if let Some(ref add_str) = s.addition {
                detail = add_str.clone();
            }
            println!(
                "{:<4} {:<18} {:<10} {:<10} {}",
                s.id, s.mount_path, s.driver, status_str, detail
            );
        }
    }
    pause();
    Ok(())
}
