use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::DbPool;
use crate::auth::{hash_password, rand_string};

pub const ROLE_ADMIN: i32 = 2;
pub const PERM_ALLOW_EMPTY_PASSWORD: i32 = 9;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub pwd_hash: String,
    #[serde(skip_serializing)]
    pub pwd_ts: i64,
    pub local_path: String,
    pub role: i32,
    pub disabled: bool,
    pub permission: i32,
    #[serde(default)]
    pub password_unset: bool,
    #[serde(skip_serializing)]
    pub otp_secret: Option<String>,
    #[serde(skip_serializing)]
    pub last_otp_step: i64,
    #[sqlx(default)]
    #[serde(default)]
    pub otp: bool,
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
    let invalid_admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `x_users` WHERE `role` = ? AND (`username` != 'admin' OR `disabled` != 0)").bind(ROLE_ADMIN).fetch_one(pool).await?;
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `x_users` WHERE `role` = ?")
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

pub(crate) async fn seed_admin(pool: &DbPool, home_path: &std::path::Path) -> Result<()> {
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `x_users` WHERE `role` = ?")
        .bind(ROLE_ADMIN)
        .fetch_one(pool)
        .await?;
    if admin_count == 0 {
        let initial_pwd = rand_string(16);
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        sqlx::query("INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `local_path`, `role`, `disabled`, `permission`, `password_unset`) VALUES ('admin', ?, ?, ?, ?, 0, 0, 0)")
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
    }
    Ok(())
}

pub async fn get_admin(pool: &DbPool) -> Result<Option<User>> {
    let mut user = sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `role` = ? LIMIT 1")
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
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query("UPDATE `x_users` SET `pwd_hash` = ?, `pwd_ts` = MAX(`pwd_ts` + 1, ?), `password_unset` = ? WHERE `id` = ?")
        .bind(hash_password(new_password)).bind(now_ts).bind(new_password.is_empty()).bind(user.id).execute(pool).await?;
    Ok(())
}

pub async fn set_admin_password(pool: &DbPool, new_password: &str) -> Result<()> {
    set_user_password(pool, "admin", new_password).await
}

pub async fn get_user_by_name(pool: &DbPool, username: &str) -> Result<Option<User>> {
    let mut user =
        sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `username` = ? LIMIT 1")
            .bind(username)
            .fetch_optional(pool)
            .await?;
    if let Some(user) = &mut user {
        user.update_otp();
    }
    Ok(user)
}

pub async fn get_all_users(pool: &DbPool) -> Result<Vec<User>> {
    let mut users = sqlx::query_as::<_, User>("SELECT * FROM `x_users` ORDER BY `id` ASC")
        .fetch_all(pool)
        .await?;
    for user in &mut users {
        user.update_otp();
    }
    Ok(users)
}

pub async fn get_user_by_id(pool: &DbPool, id: i64) -> Result<Option<User>> {
    let mut user = sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `id` = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if let Some(user) = &mut user {
        user.update_otp();
    }
    Ok(user)
}

pub async fn delete_user(pool: &DbPool, user_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM `x_users` WHERE `id` = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn enable_user_2fa(
    pool: &DbPool,
    user_id: i64,
    secret: &str,
    accepted_step: i64,
) -> Result<()> {
    let result = sqlx::query("UPDATE `x_users` SET `otp_secret` = ?, `last_otp_step` = ? WHERE `id` = ? AND (`otp_secret` IS NULL OR TRIM(`otp_secret`) = '')")
        .bind(secret).bind(accepted_step).bind(user_id).execute(pool).await?;
    if result.rows_affected() != 1 {
        bail!("user not found or 2FA is already enabled");
    }
    Ok(())
}

pub async fn cancel_user_2fa(pool: &DbPool, user_id: i64) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `otp_secret` = '', `last_otp_step` = -1 WHERE `id` = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_user_permission(pool: &DbPool, user_id: i64, permission: i32) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `permission` = ? WHERE `id` = ?")
        .bind(permission)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_user_dir(pool: &DbPool, user_id: i64, local_path: &str) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `local_path` = ? WHERE `id` = ?")
        .bind(local_path)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create_user_direct(
    pool: &DbPool,
    username: &str,
    password: &str,
    role: i32,
    local_path: Option<&str>,
    permission: i32,
    disabled: bool,
) -> Result<i64> {
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let mut tx = pool.begin().await?;
    let result = sqlx::query("INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `local_path`, `role`, `disabled`, `permission`, `password_unset`) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(username).bind(hash_password(password)).bind(now_ts).bind(local_path.context("local_path is required")?).bind(role).bind(if disabled { 1 } else { 0 }).bind(permission).bind(password.is_empty()).execute(&mut *tx).await?;
    let new_id = result.last_insert_rowid();
    tx.commit().await?;
    Ok(new_id)
}
