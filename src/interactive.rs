use crate::auth;
use crate::db::{self, DbPool, PERM_ALLOW_EMPTY_PASSWORD, ROLE_ADMIN, User};
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
    if role == ROLE_ADMIN {
        return "完全控制 (管理员)".to_string();
    }

    let file_perm = perm & !(1 << PERM_ALLOW_EMPTY_PASSWORD);
    let allow_empty = perm & (1 << PERM_ALLOW_EMPTY_PASSWORD) != 0;
    let base = match file_perm {
        504 => "全部文件权限".to_string(),
        0 => "只读浏览".to_string(),
        _ => {
            let names: Vec<_> = PERM_ITEMS
                .iter()
                .filter(|item| perm & (1 << item.bit) != 0)
                .map(|item| item.name)
                .collect();
            if names.is_empty() {
                "无文件权限".to_string()
            } else {
                names.join(",")
            }
        }
    };

    if allow_empty {
        format!("{base},免密")
    } else {
        base
    }
}

fn prompt(label: &str) -> io::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut input = String::new();
    let n = io::stdin().read_line(&mut input)?;
    if n == 0 {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    Ok(input.trim().to_string())
}

fn prompt_default(label: &str, default: &str) -> io::Result<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    let n = io::stdin().read_line(&mut input)?;
    if n == 0 {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    let value = input.trim();
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
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

fn resolve_local_path(input: &str, default_home: &str) -> PathBuf {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return PathBuf::from(default_home);
    }
    if trimmed.starts_with(default_home) {
        return PathBuf::from(trimmed);
    }
    if trimmed.starts_with('~') {
        return PathBuf::from(default_home)
            .join(trimmed.trim_start_matches('~').trim_start_matches('/'));
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    PathBuf::from(default_home).join(trimmed.trim_start_matches('/'))
}

fn parse_perm_input(input: &str) -> Option<i32> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("all") {
        return Some(
            PERM_ITEMS
                .iter()
                .fold(0, |perm, item| perm | (1 << item.bit)),
        );
    }
    if trimmed.eq_ignore_ascii_case("none") || trimmed == "0" {
        return Some(0);
    }

    let mut perm = 0;
    if trimmed.contains(|c: char| c.is_whitespace() || c == ',') {
        for part in trimmed.split(|c: char| c.is_whitespace() || c == ',') {
            if part.is_empty() {
                continue;
            }
            let num = part.parse::<usize>().ok()?;
            if !(1..=PERM_ITEMS.len()).contains(&num) {
                return None;
            }
            perm |= 1 << PERM_ITEMS[num - 1].bit;
        }
    } else {
        for ch in trimmed.chars() {
            let num = ch.to_digit(10)? as usize;
            if !(1..=PERM_ITEMS.len()).contains(&num) {
                return None;
            }
            perm |= 1 << PERM_ITEMS[num - 1].bit;
        }
    }
    Some(perm)
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
        let input = prompt("请输入权限序号 (如 1 2 3，all 全选，0 无权限，q 返回): ")?;
        if input.eq_ignore_ascii_case("q") || input == "cancel" {
            return Ok(None);
        }
        let Some(mut perm) = parse_perm_input(&input) else {
            println!("输入无效，请输入 1-{} 的序号、all 或 0。", PERM_ITEMS.len());
            continue;
        };

        let default = if allow_empty_default { "y" } else { "n" };
        let allow_empty = prompt_default("允许免密登录？(y/n)", default)?;
        if allow_empty.eq_ignore_ascii_case("q") || allow_empty == "cancel" {
            return Ok(None);
        }
        if allow_empty.eq_ignore_ascii_case("y") {
            perm |= 1 << PERM_ALLOW_EMPTY_PASSWORD;
        }
        return Ok(Some(perm));
    }
}

pub async fn run_interactive_console(pool: &DbPool, _data_dir: &Path) -> Result<()> {
    loop {
        println!();
        println!("==================================================");
        println!("                Rulist 控制台管理");
        println!("==================================================");
        println!(" 1. 添加用户");
        println!(" 2. 修改密码");
        println!(" 3. 设置目录");
        println!(" 4. 设置权限");
        println!(" 5. 重置密码");
        println!(" 6. 绑定 2FA");
        println!(" 7. 解绑 2FA");
        println!(" 8. 删除用户");
        println!(" 0. 退出");
        println!("==================================================");

        let choice = prompt("请选择操作 [0-8]: ")?;
        let handled = match choice.as_str() {
            "1" => {
                action_add_user(pool).await?;
                true
            }
            "2" => {
                action_change_password(pool).await?;
                true
            }
            "3" => {
                action_set_user_dir(pool).await?;
                true
            }
            "4" => {
                action_set_user_permissions(pool).await?;
                true
            }
            "5" => {
                action_reset_password(pool).await?;
                true
            }
            "6" => {
                action_bind_2fa(pool).await?;
                true
            }
            "7" => {
                action_unbind_2fa(pool).await?;
                true
            }
            "8" => {
                action_delete_user(pool).await?;
                true
            }
            "0" | "q" | "exit" => {
                println!("已退出 Rulist 交互控制台。");
                break;
            }
            _ => {
                println!("无效输入，请重新选择。");
                false
            }
        };

        if handled {
            pause();
        }
    }

    Ok(())
}

async fn select_user(pool: &DbPool, general_only: bool) -> Result<Option<User>> {
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
    println!("\n>>> 添加用户");
    let username = prompt("请输入用户名: ")?;
    if username.is_empty() || username.eq_ignore_ascii_case("q") {
        return Ok(());
    }
    if db::get_user_by_name(pool, &username).await?.is_some() {
        println!("错误: 用户 '{username}' 已存在。");
        return Ok(());
    }

    let default_home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let input = prompt_default("本地目录路径", &default_home)?;
    let path = resolve_local_path(&input, &default_home);
    if !path.exists() {
        println!("目录不存在: {path:?}");
        return Ok(());
    }
    if !path.is_dir() {
        println!("路径不是目录: {path:?}");
        return Ok(());
    }
    let local_path = match path.canonicalize() {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(error) => {
            println!("路径无效: {error}");
            return Ok(());
        }
    };

    let Some(permission) = select_permissions(false)? else {
        println!("操作已取消。");
        return Ok(());
    };

    let password = loop {
        let password = prompt_password("请输入登录密码 (若已选免密可留空): ")?;
        if password.is_empty() {
            if permission & (1 << PERM_ALLOW_EMPTY_PASSWORD) != 0 {
                break password;
            }
            println!("未开启免密登录，密码不能为空。");
            continue;
        }
        if !auth::valid_password(&password) {
            println!("错误: 密码长度需在 8 到 128 位之间。");
            continue;
        }
        if password != prompt_password("请再次输入确认密码: ")? {
            println!("错误: 两次输入的密码不一致。");
            continue;
        }
        break password;
    };

    let id = db::create_user_direct(
        pool,
        &username,
        &password,
        0,
        Some(&local_path),
        permission,
        false,
    )
    .await?;

    println!("\n用户 '{username}' 创建成功！");
    println!("  ID: {id}");
    println!("  目录: {local_path}");
    println!("  权限: {}", format_permissions(0, permission));
    Ok(())
}

async fn action_change_password(pool: &DbPool) -> Result<()> {
    println!("\n>>> 修改密码");
    let Some(user) = select_user(pool, false).await? else {
        return Ok(());
    };

    let password = loop {
        let password = prompt_password("请输入新密码 (普通用户可留空设置免密): ")?;
        if password.is_empty() {
            if user.is_admin() {
                println!("错误: 管理员账户不允许空密码。");
                continue;
            }
            if prompt_default("确认设置为空密码并开启免密登录？(y/N)", "n")?
                .eq_ignore_ascii_case("y")
            {
                db::set_user_permission(
                    pool,
                    user.id,
                    user.permission | (1 << PERM_ALLOW_EMPTY_PASSWORD),
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
            println!("错误: 两次输入的密码不一致。");
            continue;
        }
        break password;
    };

    db::set_user_password(pool, &user.username, &password).await?;
    println!("用户 '{}' 的密码已更新。", user.username);
    Ok(())
}

async fn action_set_user_dir(pool: &DbPool) -> Result<()> {
    println!("\n>>> 设置目录");
    let Some(user) = select_user(pool, false).await? else {
        return Ok(());
    };

    let default_home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let input = prompt_default("请输入新的本地目录路径", &default_home)?;
    let path = resolve_local_path(&input, &default_home);
    if !path.exists() {
        println!("目录不存在: {path:?}");
        return Ok(());
    }
    if !path.is_dir() {
        println!("路径不是目录: {path:?}");
        return Ok(());
    }

    let path = path.canonicalize()?.to_string_lossy().into_owned();
    db::set_user_dir(pool, user.id, &path).await?;
    println!("用户 '{}' 的目录已更新为: {path}", user.username);
    Ok(())
}

async fn action_set_user_permissions(pool: &DbPool) -> Result<()> {
    println!("\n>>> 设置权限");
    let Some(user) = select_user(pool, true).await? else {
        return Ok(());
    };

    println!(
        "当前权限: {}",
        format_permissions(user.role, user.permission)
    );
    let allow_empty = user.permission & (1 << PERM_ALLOW_EMPTY_PASSWORD) != 0;
    let Some(permission) = select_permissions(allow_empty)? else {
        println!("操作已取消。");
        return Ok(());
    };

    db::set_user_permission(pool, user.id, permission).await?;
    println!(
        "用户 '{}' 的权限已更新为: {}",
        user.username,
        format_permissions(user.role, permission)
    );
    Ok(())
}

async fn action_reset_password(pool: &DbPool) -> Result<()> {
    println!("\n>>> 重置密码");
    let Some(user) = select_user(pool, false).await? else {
        return Ok(());
    };

    if !prompt_default(
        &format!("确定要重置用户 '{}' 的密码吗？(y/N)", user.username),
        "n",
    )?
    .eq_ignore_ascii_case("y")
    {
        println!("操作已取消。");
        return Ok(());
    }

    let password = auth::rand_string(16);
    db::set_user_password(pool, &user.username, &password).await?;
    println!("密码已重置。2FA 状态保持不变。");
    println!("用户名: {}", user.username);
    println!("新密码: {password}");
    Ok(())
}

async fn action_bind_2fa(pool: &DbPool) -> Result<()> {
    println!("\n>>> 绑定 2FA");
    let Some(user) = select_user(pool, false).await? else {
        return Ok(());
    };
    if user.otp {
        println!("用户 '{}' 已启用 2FA。", user.username);
        return Ok(());
    }

    let secret = auth::generate_otp_secret();
    println!("\n请在身份验证器中手动添加以下信息：");
    println!("密钥: {secret}");
    println!("类型: TOTP");
    println!("位数: 6");
    println!("周期: 30 秒");
    println!("算法: SHA1");
    println!("\n只有验证码验证成功后才会保存此密钥。");

    let code = prompt("请输入身份验证器生成的 6 位验证码 (留空取消): ")?;
    if code.is_empty() || code.eq_ignore_ascii_case("q") {
        println!("操作已取消，2FA 未启用。");
        return Ok(());
    }

    let Some(step) = auth::matching_totp_step(&secret, &code) else {
        println!("验证码无效，2FA 未启用。");
        return Ok(());
    };

    db::enable_user_2fa(pool, user.id, &secret, step).await?;
    println!("用户 '{}' 的 2FA 已启用。", user.username);
    Ok(())
}

async fn action_unbind_2fa(pool: &DbPool) -> Result<()> {
    println!("\n>>> 解绑 2FA");
    let Some(user) = select_user(pool, false).await? else {
        return Ok(());
    };
    if !user.otp {
        println!("用户 '{}' 未启用 2FA。", user.username);
        return Ok(());
    }
    if !prompt_default(
        &format!("确定要解绑用户 '{}' 的 2FA 吗？(y/N)", user.username),
        "n",
    )?
    .eq_ignore_ascii_case("y")
    {
        println!("操作已取消。");
        return Ok(());
    }

    db::cancel_user_2fa(pool, user.id).await?;
    println!("用户 '{}' 的 2FA 已解绑。", user.username);
    Ok(())
}

async fn action_delete_user(pool: &DbPool) -> Result<()> {
    println!("\n>>> 删除用户");
    let Some(user) = select_user(pool, true).await? else {
        return Ok(());
    };
    if !prompt_default(
        &format!("确定要彻底删除用户 '{}' 吗？(y/N)", user.username),
        "n",
    )?
    .eq_ignore_ascii_case("y")
    {
        println!("操作已取消。");
        return Ok(());
    }

    db::delete_user(pool, user.id).await?;
    println!("用户 '{}' 已删除。", user.username);
    Ok(())
}
