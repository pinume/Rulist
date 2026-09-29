use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::DbPool;
use crate::auth::{hash_password, rand_string};

pub const ROLE_ADMIN: i32 = 2;
pub const PERM_ALLOW_EMPTY_PASSWORD: i32 = 9;

fn canonical_local_path(path: &str) -> Result<String> {
    let path = Path::new(path.trim());
    if !path.is_absolute() {
        bail!("local path must be absolute");
    }
    let path = path.canonicalize()?;
    if !path.is_dir() {
        bail!("local path must be a directory");
    }
    Ok(path.to_string_lossy().into_owned())
}

fn validate_username(username: &str) -> Result<()> {
    let username = username.trim();
    if username.is_empty() || username.chars().count() > 64 {
        bail!("username must be 1 to 64 characters");
    }
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub pwd_hash: String,
    pub pwd_ts: i64,
    pub local_path: String,
    pub role: i32,
    pub disabled: bool,
    pub permission: i32,
    pub password_unset: bool,
    pub otp_secret: Option<String>,
    pub last_otp_step: i64,
    #[sqlx(default)]
    pub otp: bool,
}

#[derive(Debug, Serialize)]
pub struct SessionUser {
    pub id: i64,
    pub username: String,
    pub role: i32,
    pub permission: i32,
    pub otp: bool,
}

impl From<&User> for SessionUser {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            role: user.role,
            permission: user.permission,
            otp: user.otp,
        }
    }
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }

    pub fn update_otp(&mut self) {
        self.otp = self
            .otp_secret
            .as_deref()
            .map(|secret| !secret.trim().is_empty())
            .unwrap_or(false);
    }
}

pub(crate) async fn validate_admin_invariants(pool: &DbPool) -> Result<()> {
    let invalid_admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `users` WHERE `role` = ? AND (`username` != 'admin' OR `disabled` != 0)").bind(ROLE_ADMIN).fetch_one(pool).await?;
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `users` WHERE `role` = ?")
        .bind(ROLE_ADMIN)
        .fetch_one(pool)
        .await?;
    if invalid_admins > 0 || admin_count > 1 {
        bail!("database must contain at most one enabled administrator named admin");
    }
    if admin_count == 0 && get_user_by_name(pool, "admin").await?.is_some() {
        bail!("username admin is already assigned to a non-administrator");
    }
    Ok(())
}

fn initial_home_path() -> Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .context("HOME is not set; cannot initialize the administrator directory")?;
    let home = home
        .canonicalize()
        .with_context(|| format!("failed to resolve current user's HOME directory: {home:?}"))?;
    if !home.is_dir() {
        bail!("current user's HOME is not a directory: {home:?}");
    }
    Ok(home)
}

pub(crate) async fn seed_admin(pool: &DbPool) -> Result<()> {
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `users` WHERE `role` = ?")
        .bind(ROLE_ADMIN)
        .fetch_one(pool)
        .await?;
    if admin_count != 0 {
        return Ok(());
    }

    let home_path = initial_home_path()?;
    let initial_pwd = rand_string(16);
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query("INSERT INTO `users` (`username`, `pwd_hash`, `pwd_ts`, `local_path`, `role`, `disabled`, `permission`, `password_unset`) VALUES ('admin', ?, ?, ?, ?, 0, 0, 0)")
        .bind(hash_password(&initial_pwd)).bind(now_ts).bind(home_path.to_string_lossy().into_owned()).bind(ROLE_ADMIN).execute(pool).await?;
    println!(
        "\n==================================================================\n\
         Initial admin user created:\n\
         Username: admin\n\
         Password: {}\n\
         Save this password and change it after first login.\n\
         You can reset it later from the Rulist interactive console.\n\
         ==================================================================\n",
        initial_pwd
    );
    Ok(())
}

pub async fn get_admin(pool: &DbPool) -> Result<Option<User>> {
    let mut user = sqlx::query_as::<_, User>("SELECT * FROM `users` WHERE `role` = ? LIMIT 1")
        .bind(ROLE_ADMIN)
        .fetch_optional(pool)
        .await?;
    if let Some(user) = &mut user {
        user.update_otp();
    }
    Ok(user)
}

pub async fn set_user_password(pool: &DbPool, username: &str, new_password: &str) -> Result<()> {
    let user = get_user_by_name(pool, username)
        .await?
        .context("user not found")?;
    set_user_password_and_permission(pool, user.id, new_password, user.permission).await
}

pub async fn set_user_password_and_permission(
    pool: &DbPool,
    user_id: i64,
    new_password: &str,
    permission: i32,
) -> Result<()> {
    let user = get_user_by_id(pool, user_id)
        .await?
        .context("user not found")?;
    if user.is_admin() && permission != user.permission {
        bail!("administrator permissions cannot be changed");
    }
    crate::auth::validate_password(new_password, user.is_admin(), permission)
        .map_err(anyhow::Error::msg)?;
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query("UPDATE `users` SET `pwd_hash` = ?, `pwd_ts` = MAX(`pwd_ts` + 1, ?), `password_unset` = ?, `permission` = ? WHERE `id` = ?")
        .bind(hash_password(new_password)).bind(now_ts).bind(new_password.is_empty()).bind(permission).bind(user.id).execute(pool).await?;
    Ok(())
}

pub async fn set_admin_password(pool: &DbPool, new_password: &str) -> Result<()> {
    set_user_password(pool, "admin", new_password).await
}

pub async fn get_user_by_name(pool: &DbPool, username: &str) -> Result<Option<User>> {
    let mut user = sqlx::query_as::<_, User>("SELECT * FROM `users` WHERE `username` = ? LIMIT 1")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    if let Some(user) = &mut user {
        user.update_otp();
    }
    Ok(user)
}

pub async fn get_all_users(pool: &DbPool) -> Result<Vec<User>> {
    let mut users = sqlx::query_as::<_, User>("SELECT * FROM `users` ORDER BY `id` ASC")
        .fetch_all(pool)
        .await?;
    for user in &mut users {
        user.update_otp();
    }
    Ok(users)
}

pub async fn get_user_by_id(pool: &DbPool, id: i64) -> Result<Option<User>> {
    let mut user = sqlx::query_as::<_, User>("SELECT * FROM `users` WHERE `id` = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if let Some(user) = &mut user {
        user.update_otp();
    }
    Ok(user)
}

pub async fn delete_user(pool: &DbPool, user_id: i64) -> Result<()> {
    let user = get_user_by_id(pool, user_id)
        .await?
        .context("user not found")?;
    if user.is_admin() {
        bail!("administrator cannot be deleted");
    }
    let result = sqlx::query("DELETE FROM `users` WHERE `id` = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() != 1 {
        bail!("user not found");
    }
    Ok(())
}

pub async fn accept_otp_step(pool: &DbPool, user_id: i64, step: i64) -> Result<bool> {
    Ok(
        sqlx::query(
            "UPDATE `users` SET `last_otp_step` = ? WHERE `id` = ? AND `last_otp_step` < ?",
        )
        .bind(step)
        .bind(user_id)
        .bind(step)
        .execute(pool)
        .await?
        .rows_affected()
            == 1,
    )
}

pub async fn set_user_disabled(pool: &DbPool, user_id: i64, disabled: bool) -> Result<()> {
    let current = get_user_by_id(pool, user_id)
        .await?
        .context("user not found")?;
    if current.is_admin() {
        bail!("administrator cannot be disabled");
    }
    let result = if disabled {
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        sqlx::query(
            "UPDATE `users` SET `disabled` = 1, `pwd_ts` = MAX(`pwd_ts` + 1, ?) WHERE `id` = ?",
        )
        .bind(now_ts)
        .bind(user_id)
        .execute(pool)
        .await?
    } else {
        sqlx::query("UPDATE `users` SET `disabled` = 0 WHERE `id` = ?")
            .bind(user_id)
            .execute(pool)
            .await?
    };
    if result.rows_affected() != 1 {
        bail!("user not found");
    }
    Ok(())
}

pub async fn enable_user_2fa(
    pool: &DbPool,
    user_id: i64,
    secret: &str,
    accepted_step: i64,
) -> Result<()> {
    let result = sqlx::query("UPDATE `users` SET `otp_secret` = ?, `last_otp_step` = ? WHERE `id` = ? AND (`otp_secret` IS NULL OR TRIM(`otp_secret`) = '')")
        .bind(secret).bind(accepted_step).bind(user_id).execute(pool).await?;
    if result.rows_affected() != 1 {
        bail!("user not found or 2FA is already enabled");
    }
    Ok(())
}

pub async fn disable_user_2fa(pool: &DbPool, user_id: i64) -> Result<()> {
    let result =
        sqlx::query("UPDATE `users` SET `otp_secret` = '', `last_otp_step` = -1 WHERE `id` = ?")
            .bind(user_id)
            .execute(pool)
            .await?;
    if result.rows_affected() != 1 {
        bail!("user not found");
    }
    Ok(())
}

pub async fn set_user_permissions(pool: &DbPool, user_id: i64, permission: i32) -> Result<()> {
    let user = get_user_by_id(pool, user_id)
        .await?
        .context("user not found")?;
    if user.is_admin() {
        bail!("administrator permissions cannot be changed");
    }
    if user.password_unset && permission & (1 << PERM_ALLOW_EMPTY_PASSWORD) == 0 {
        bail!("set a non-empty password before disabling passwordless login");
    }
    let result = sqlx::query("UPDATE `users` SET `permission` = ? WHERE `id` = ?")
        .bind(permission)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() != 1 {
        bail!("user not found");
    }
    Ok(())
}

pub async fn set_user_local_path(pool: &DbPool, user_id: i64, local_path: &str) -> Result<()> {
    let local_path = canonical_local_path(local_path)?;
    let result = sqlx::query("UPDATE `users` SET `local_path` = ? WHERE `id` = ?")
        .bind(local_path)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() != 1 {
        bail!("user not found");
    }
    Ok(())
}

pub async fn create_user(
    pool: &DbPool,
    username: &str,
    password: &str,
    role: i32,
    local_path: Option<&str>,
    permission: i32,
    disabled: bool,
) -> Result<i64> {
    validate_username(username)?;
    if role != 0 {
        bail!("only normal users can be created");
    }
    let local_path = canonical_local_path(local_path.context("local_path is required")?)?;
    crate::auth::validate_password(password, false, permission).map_err(anyhow::Error::msg)?;
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let mut tx = pool.begin().await?;
    let result = sqlx::query("INSERT INTO `users` (`username`, `pwd_hash`, `pwd_ts`, `local_path`, `role`, `disabled`, `permission`, `password_unset`) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(username.trim()).bind(hash_password(password)).bind(now_ts).bind(local_path).bind(role).bind(if disabled { 1 } else { 0 }).bind(permission).bind(password.is_empty()).execute(&mut *tx).await?;
    let new_id = result.last_insert_rowid();
    tx.commit().await?;
    Ok(new_id)
}
