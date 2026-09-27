use crate::auth;
use crate::db::{self, DbPool};
use crate::model;
use anyhow::Result;
use std::io::{self, IsTerminal, Write};
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
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    Ok(input.trim().to_string())
}

fn prompt_default(label: &str, default: &str) -> io::Result<String> {
    print!("{} [{}]: ", label, default);
    io::stdout().flush()?;
    let mut input = String::new();
    let n = io::stdin().read_line(&mut input)?;
    if n == 0 {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn prompt_password(label: &str) -> io::Result<String> {
    if io::stdin().is_terminal() {
        rpassword::prompt_password(label)
    } else {
        prompt(label)
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
    if p.is_absolute() {
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

fn select_permissions(allow_empty_default: bool) -> io::Result<Option<i32>> {
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
        if let Some(mut perm) = parse_perm_input(trimmed) {
            let default = if allow_empty_default { "y" } else { "n" };
            let allow_empty = prompt_default("允许免密登录？(y/n)", default)?;
            if allow_empty.eq_ignore_ascii_case("q") || allow_empty == "cancel" {
                return Ok(None);
            }
            if allow_empty.eq_ignore_ascii_case("y") {
                perm |= 1 << model::PERM_ALLOW_EMPTY_PASSWORD;
            }
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
        println!(" 1. 添加新用户");
        println!(" 2. 修改登录密码");
        println!(" 3. 设置本地存储目录");
        println!(" 4. 设置用户操作权限");
        println!(" 5. 重置密码为随机字符串并解绑 2FA");
        println!(" 6. 解绑 2FA");
        println!(" 7. 删除此用户");
        println!(" 8. 查看存储挂载点");
        println!(" 0. 退出控制台");
        println!("==================================================");

        let choice = prompt("请选择操作 [0-8]: ")?;
        match choice.as_str() {
            "1" => {
                action_add_user(pool).await?;
                pause();
            }
            "2" => {
                action_change_password(pool).await?;
            }
            "3" => {
                action_set_user_dir(pool).await?;
            }
            "4" => {
                action_set_user_permissions(pool).await?;
            }
            "5" => {
                action_reset_password_and_2fa(pool).await?;
            }
            "6" => {
                action_unbind_2fa(pool).await?;
            }
            "7" => {
                action_delete_user(pool).await?;
            }
            "8" => {
                action_list_storages(pool).await?;
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

async fn select_user(pool: &DbPool, general_only: bool) -> Result<Option<model::User>> {
    let mut users = db::get_all_users(pool).await?;
    if general_only {
        users.retain(|user| !user.is_admin());
    }
    println!();
    println!("用户列表:");
    if users.is_empty() {
        println!("(暂无用户)");
        return Ok(None);
    }
    for (index, user) in users.iter().enumerate() {
        println!(
            " {}. {} ({}, 2FA: {})",
            index + 1,
            user.username,
            if user.is_admin() {
                "管理员"
            } else {
                "普通用户"
            },
            if user.otp { "已启用" } else { "未启用" }
        );
    }
    loop {
        let input = prompt("请选择用户序号 (0、留空或 q 返回): ")?;
        if input.is_empty() || input == "0" || input.eq_ignore_ascii_case("q") {
            return Ok(None);
        }
        match input.parse::<usize>() {
            Ok(index) if (1..=users.len()).contains(&index) => {
                return Ok(Some(users[index - 1].clone()));
            }
            _ => println!("输入无效，请输入 1-{} 的序号。", users.len()),
        }
    }
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
    if !path.is_dir() {
        println!("路径不是目录: {:?}", path);
        return Ok(());
    }
    let canon = match path.canonicalize() {
        Ok(c) => c.to_string_lossy().into_owned(),
        Err(e) => {
            println!("路径无效: {}", e);
            return Ok(());
        }
    };
    let dir_input = Some(canon);

    let Some(perm) = select_permissions(false)? else {
        println!("操作已取消。");
        return Ok(());
    };

    let password = loop {
        let pwd = prompt_password("请输入登录密码 (若已选免密登录可直接回车留空): ")?;
        if pwd.is_empty() {
            if (perm & (1 << model::PERM_ALLOW_EMPTY_PASSWORD)) != 0 {
                break String::new();
            } else {
                println!("用户未开启「免密登录」权限，密码不能为空，请输入密码。");
                continue;
            }
        }
        if !auth::valid_password(&pwd) {
            println!("错误: 密码长度需在 8 到 128 位之间。");
            continue;
        }
        let confirm = prompt_password("请再次输入以确认密码: ")?;
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

async fn action_change_password(pool: &DbPool) -> Result<()> {
    println!();
    let Some(user) = select_user(pool, false).await? else {
        return Ok(());
    };
    let password = loop {
        let password = prompt_password("请输入新密码 (直接回车表示留空免密): ")?;
        if password.is_empty() {
            if user.is_admin() {
                println!("错误: 管理员账户不允许设置为空密码。");
                continue;
            }
            if prompt_default(
                "检测到密码为空，是否确认设置为空密码并开启免密登录权限？(y/N)",
                "n",
            )?
            .eq_ignore_ascii_case("y")
            {
                db::set_user_permission(
                    pool,
                    user.id,
                    user.permission | (1 << model::PERM_ALLOW_EMPTY_PASSWORD),
                )
                .await?;
                break password;
            }
            println!("操作已取消，请重新输入密码。");
            continue;
        }
        if !auth::valid_password(&password) {
            println!("错误: 密码长度需在 8 到 128 位之间。");
            continue;
        }
        if password != prompt_password("请再次输入确认密码: ")? {
            println!("错误: 两次输入的密码不一致，请重新输入。");
            continue;
        }
        break password;
    };
    db::set_user_password(pool, &user.username, &password, false).await?;
    println!("用户 '{}' 的密码已成功更新！", user.username);
    pause();
    Ok(())
}

async fn action_set_user_dir(pool: &DbPool) -> Result<()> {
    let Some(user) = select_user(pool, true).await? else {
        return Ok(());
    };
    let default_home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let input = prompt_default("请输入新的本地目录路径", &default_home)?;
    let path = resolve_storage_path(&input, &default_home);
    if !path.exists() {
        if !prompt_default(&format!("目录 {:?} 不存在，是否创建？ (y/n)", path), "y")?
            .eq_ignore_ascii_case("y")
        {
            println!("操作已取消。");
            pause();
            return Ok(());
        }
        if let Err(error) = std::fs::create_dir_all(&path) {
            println!("创建目录失败: {}", error);
            pause();
            return Ok(());
        }
    }
    if !path.is_dir() {
        println!("路径不是目录: {:?}", path);
        pause();
        return Ok(());
    }
    let path = match path.canonicalize() {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(error) => {
            println!("路径无效: {}", error);
            pause();
            return Ok(());
        }
    };
    db::set_user_dir(pool, user.id, &path).await?;
    println!("用户 '{}' 的存储目录已成功更新为: {}", user.username, path);
    pause();
    Ok(())
}

async fn action_set_user_permissions(pool: &DbPool) -> Result<()> {
    let Some(user) = select_user(pool, true).await? else {
        return Ok(());
    };
    println!(
        "用户 '{}' 当前权限: {}",
        user.username,
        format_permissions(user.role, user.permission)
    );
    if let Some(permission) =
        select_permissions(user.permission & (1 << model::PERM_ALLOW_EMPTY_PASSWORD) != 0)?
    {
        db::set_user_permission(pool, user.id, permission).await?;
        println!(
            "用户 '{}' 的权限已更新为: {}",
            user.username,
            format_permissions(user.role, permission)
        );
    } else {
        println!("操作已取消，权限未作更改。");
    }
    pause();
    Ok(())
}

async fn action_delete_user(pool: &DbPool) -> Result<()> {
    let Some(user) = select_user(pool, true).await? else {
        return Ok(());
    };
    if prompt_default(
        &format!(
            "确定要彻底删除用户 '{}' 及其存储挂载点吗？(y/N)",
            user.username
        ),
        "n",
    )?
    .eq_ignore_ascii_case("y")
    {
        db::delete_user(pool, user.id).await?;
        println!("用户 '{}' 已成功删除。", user.username);
    } else {
        println!("操作已取消。");
    }
    pause();
    Ok(())
}

async fn action_reset_password_and_2fa(pool: &DbPool) -> Result<()> {
    let Some(user) = select_user(pool, false).await? else {
        return Ok(());
    };
    if prompt_default(
        &format!(
            "确定要重置用户 '{}' 的密码并解绑 2FA 吗？(y/N)",
            user.username
        ),
        "n",
    )?
    .eq_ignore_ascii_case("y")
    {
        let password = auth::rand_string(16);
        db::set_user_password(pool, &user.username, &password, true).await?;
        println!("密码已重置，且 2FA 已解绑！");
        println!("用户名: {}", user.username);
        println!("新密码: {}", password);
    } else {
        println!("操作已取消。");
    }
    pause();
    Ok(())
}

async fn action_unbind_2fa(pool: &DbPool) -> Result<()> {
    let Some(user) = select_user(pool, false).await? else {
        return Ok(());
    };
    db::cancel_user_2fa(pool, user.id).await?;
    println!(
        "用户 '{}' 的双因素认证 (2FA) 已成功解除绑定。",
        user.username
    );
    pause();
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

#[cfg(test)]
mod tests {
    use super::{parse_perm_input, resolve_storage_path};
    use crate::model;
    use std::path::PathBuf;

    #[test]
    fn preserves_absolute_storage_paths_and_all_excludes_passwordless() {
        assert_eq!(
            resolve_storage_path("/var/lib/rulist/new-user", "/home/rulist"),
            PathBuf::from("/var/lib/rulist/new-user")
        );
        assert_eq!(
            resolve_storage_path("uploads", "/home/rulist"),
            PathBuf::from("/home/rulist/uploads")
        );

        let all = parse_perm_input("all").unwrap();
        assert_eq!(all, 504);
        assert_eq!(all & (1 << model::PERM_ALLOW_EMPTY_PASSWORD), 0);
    }
}
