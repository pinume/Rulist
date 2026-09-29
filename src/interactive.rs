use crate::{
    auth,
    config::Config,
    db::{self, DbPool, PERM_ALLOW_EMPTY_PASSWORD, User},
};
use anyhow::Result;
use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

const PERMS: &[(i32, &str)] = &[
    (3, "新建与上传"),
    (4, "重命名"),
    (5, "移动"),
    (6, "复制"),
    (7, "删除"),
    (8, "覆盖"),
];
pub fn format_permissions(role: i32, permission: i32) -> String {
    if role == db::ROLE_ADMIN {
        return "完全控制 (管理员)".into();
    }
    let names: Vec<_> = PERMS
        .iter()
        .filter(|(bit, _)| permission & (1 << bit) != 0)
        .map(|(_, name)| *name)
        .collect();
    let mut text = if names.is_empty() {
        "只读浏览".into()
    } else {
        names.join(",")
    };
    if permission & (1 << PERM_ALLOW_EMPTY_PASSWORD) != 0 {
        text.push_str(",免密");
    }
    text
}
fn prompt(label: &str) -> io::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    Ok(input.trim().into())
}
fn password(label: &str) -> io::Result<String> {
    if io::stdin().is_terminal() {
        rpassword::prompt_password(label)
    } else {
        prompt(label)
    }
}
fn local_path(input: &str) -> Result<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let input = input.trim();
    let path = if input == "~" || input.is_empty() {
        PathBuf::from(home)
    } else if let Some(rest) = input.strip_prefix("~/") {
        PathBuf::from(home).join(rest)
    } else if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        PathBuf::from(home).join(input)
    };
    let path = path.canonicalize()?;
    anyhow::ensure!(path.is_dir(), "路径不是目录");
    Ok(path.to_string_lossy().into_owned())
}
fn choose_permissions(default_empty: bool) -> io::Result<Option<i32>> {
    println!("权限: 1上传 2重命名 3移动 4复制 5删除 6覆盖；all/0");
    let input = prompt("选择 (q 返回): ")?;
    if input.eq_ignore_ascii_case("q") {
        return Ok(None);
    }
    let mut permission = if input.eq_ignore_ascii_case("all") {
        PERMS.iter().fold(0, |value, (bit, _)| value | (1 << bit))
    } else if input == "0" {
        0
    } else {
        let mut value = 0;
        for part in input.split(|ch: char| ch.is_whitespace() || ch == ',') {
            let Some((bit, _)) = part
                .parse::<usize>()
                .ok()
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| PERMS.get(i))
            else {
                return Ok(None);
            };
            value |= 1 << bit;
        }
        value
    };
    let empty = prompt("允许免密登录? (y/N): ")?;
    if empty.eq_ignore_ascii_case("y") || (empty.is_empty() && default_empty) {
        permission |= 1 << PERM_ALLOW_EMPTY_PASSWORD;
    }
    Ok(Some(permission))
}
fn users(users: &[User]) {
    for user in users {
        println!(
            "{} | {} | {} | {} | {} | 2FA:{} | {}",
            user.id,
            user.username,
            if user.is_admin() {
                "管理员"
            } else {
                "普通用户"
            },
            user.local_path,
            format_permissions(user.role, user.permission),
            if user.otp { "开" } else { "关" },
            if user.disabled { "禁用" } else { "启用" }
        );
    }
}
async fn select(pool: &DbPool) -> Result<Option<User>> {
    let all = db::get_all_users(pool).await?;
    users(&all);
    let Ok(id) = prompt("用户 ID (留空返回): ")?.parse::<i64>() else {
        return Ok(None);
    };
    Ok(all.into_iter().find(|user| user.id == id))
}
async fn set_password(pool: &DbPool, user: &mut User) -> Result<()> {
    let value = password("新密码（random 自动生成）: ")?;
    let value = if value.eq_ignore_ascii_case("random") {
        let value = auth::rand_string(16);
        println!("新密码: {value}");
        value
    } else {
        value
    };
    if value.is_empty() {
        if user.is_admin()
            || !prompt("确认开启免密并设置空密码? (y/N): ")?.eq_ignore_ascii_case("y")
        {
            return Ok(());
        }
        user.permission |= 1 << PERM_ALLOW_EMPTY_PASSWORD;
    }
    match auth::validate_password(&value, user.is_admin(), user.permission) {
        Ok(()) => {
            db::set_user_password_and_permission(pool, user.id, &value, user.permission).await?;
            *user = db::get_user_by_id(pool, user.id)
                .await?
                .expect("updated user exists");
        }
        Err(error) => println!("错误: {error}"),
    }
    Ok(())
}
async fn manage(pool: &DbPool, mut user: User) -> Result<()> {
    loop {
        println!(
            "用户: {} | 目录: {} | 权限: {} | 状态: {} | 2FA: {}",
            user.username,
            user.local_path,
            format_permissions(user.role, user.permission),
            if user.disabled { "禁用" } else { "启用" },
            if user.otp { "开" } else { "关" }
        );
        println!(
            "管理 {}: 1密码 2目录 3权限 4 2FA 5启用/禁用 6删除 0返回",
            user.username
        );
        match prompt("选择: ")?.as_str() {
            "1" => set_password(pool, &mut user).await?,
            "2" => {
                let path = local_path(&prompt("目录: ")?)?;
                db::set_user_local_path(pool, user.id, &path).await?;
                user = match db::get_user_by_id(pool, user.id).await? {
                    Some(user) => user,
                    None => break,
                };
            }
            "3" if !user.is_admin() => {
                if let Some(permission) =
                    choose_permissions(user.permission & (1 << PERM_ALLOW_EMPTY_PASSWORD) != 0)?
                {
                    if user.password_unset && permission & (1 << PERM_ALLOW_EMPTY_PASSWORD) == 0 {
                        let value = password("需设置非空密码（random 自动生成）: ")?;
                        let generated = value.eq_ignore_ascii_case("random");
                        let value = if generated {
                            let value = auth::rand_string(16);
                            println!("新密码: {value}");
                            value
                        } else {
                            value
                        };
                        if value.is_empty()
                            || auth::validate_password(&value, false, permission).is_err()
                            || (!generated && value != password("确认密码: ")?)
                        {
                            println!("密码无效或不一致");
                            continue;
                        }
                        db::set_user_password_and_permission(pool, user.id, &value, permission)
                            .await?;
                    } else {
                        db::set_user_permissions(pool, user.id, permission).await?;
                    }
                    user = match db::get_user_by_id(pool, user.id).await? {
                        Some(user) => user,
                        None => break,
                    };
                }
            }
            "4" => {
                if user.otp {
                    if prompt("确认解绑 2FA? (y/N): ")?.eq_ignore_ascii_case("y") {
                        db::disable_user_2fa(pool, user.id).await?;
                        user = match db::get_user_by_id(pool, user.id).await? {
                            Some(user) => user,
                            None => break,
                        };
                    }
                } else {
                    if !prompt("确认绑定 2FA? (y/N): ")?.eq_ignore_ascii_case("y") {
                        continue;
                    }
                    let secret = auth::generate_otp_secret();
                    println!("密钥: {secret}");
                    if let Some(step) = auth::matching_totp_step(&secret, &prompt("验证码: ")?) {
                        db::enable_user_2fa(pool, user.id, &secret, step).await?;
                        user = match db::get_user_by_id(pool, user.id).await? {
                            Some(user) => user,
                            None => break,
                        };
                    }
                }
            }
            "5" if !user.is_admin() => {
                if prompt("确认切换启用状态? (y/N): ")?.eq_ignore_ascii_case("y") {
                    user.disabled = !user.disabled;
                    db::set_user_disabled(pool, user.id, user.disabled).await?;
                    user = match db::get_user_by_id(pool, user.id).await? {
                        Some(user) => user,
                        None => break,
                    };
                }
            }
            "6" if !user.is_admin() => {
                if prompt("确认删除? (y/N): ")?.eq_ignore_ascii_case("y") {
                    db::delete_user(pool, user.id).await?;
                    break;
                }
            }
            "0" | "q" => break,
            _ => println!("无效或管理员不可执行该操作"),
        }
    }
    Ok(())
}
async fn add(pool: &DbPool) -> Result<()> {
    let username = prompt("用户名: ")?;
    if username.is_empty() {
        return Ok(());
    }
    let path = local_path(&prompt("目录: ")?)?;
    let Some(permission) = choose_permissions(false)? else {
        return Ok(());
    };
    let value = password("密码: ")?;
    if let Err(error) = auth::validate_password(&value, false, permission) {
        println!("错误: {error}");
        return Ok(());
    }
    if !value.is_empty() && value != password("确认密码: ")? {
        println!("两次密码不一致");
        return Ok(());
    }
    if !prompt("确认创建用户? (y/N): ")?.eq_ignore_ascii_case("y") {
        return Ok(());
    }
    db::create_user(pool, &username, &value, 0, Some(&path), permission, false).await?;
    Ok(())
}
async fn service_info(config: &Config, data_dir: &Path, pool: &DbPool) -> Result<()> {
    let users = db::get_all_users(pool).await?;
    let admin_dir = users
        .iter()
        .find(|user| user.is_admin())
        .map(|user| user.local_path.as_str())
        .unwrap_or("-");
    println!(
        "版本: v{}\n数据目录: {}\n数据库: {}\n监听: {}:{}\n管理员目录: {}\n用户数: {}\n配置: {}",
        env!("CARGO_PKG_VERSION"),
        data_dir.display(),
        config.resolved_db_path(data_dir).display(),
        config.scheme.address,
        config.scheme.http_port,
        admin_dir,
        users.len(),
        data_dir.join("config.json").display()
    );
    Ok(())
}
pub async fn run_interactive_console(
    pool: &DbPool,
    data_dir: &Path,
    config: &Config,
) -> Result<()> {
    loop {
        println!("\n1. 用户列表\n2. 添加用户\n3. 管理用户\n4. 服务信息\n0. 退出");
        match prompt("选择: ")?.as_str() {
            "1" => users(&db::get_all_users(pool).await?),
            "2" => add(pool).await?,
            "3" => {
                if let Some(user) = select(pool).await? {
                    manage(pool, user).await?
                }
            }
            "4" => service_info(config, data_dir, pool).await?,
            "0" | "q" | "exit" => break,
            _ => println!("无效输入"),
        }
    }
    Ok(())
}
