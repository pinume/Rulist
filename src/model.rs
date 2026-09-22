#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            code: 200,
            message: "success".to_string(),
            data: Some(data),
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

pub const ROLE_GENERAL: i32 = 0;
pub const ROLE_GUEST: i32 = 1;
pub const ROLE_ADMIN: i32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub pwd_hash: String,
    #[serde(skip_serializing)]
    pub pwd_ts: i64,
    #[serde(skip_serializing)]
    pub salt: String,
    #[serde(skip_serializing)]
    pub password: Option<String>,
    pub base_path: String,
    pub role: i32,
    pub disabled: bool,
    pub permission: i32,
    #[serde(skip_serializing)]
    pub otp_secret: Option<String>,
    pub sso_id: Option<String>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }

    pub fn can_see_hides(&self) -> bool {
        (self.permission & 1) == 1
    }

    pub fn can_access_without_password(&self) -> bool {
        ((self.permission >> 1) & 1) == 1
    }

    pub fn can_write_content(&self) -> bool {
        ((self.permission >> 3) & 1) == 1
    }

    pub fn can_rename(&self) -> bool {
        ((self.permission >> 4) & 1) == 1
    }

    pub fn can_move(&self) -> bool {
        ((self.permission >> 5) & 1) == 1
    }

    pub fn can_copy(&self) -> bool {
        ((self.permission >> 6) & 1) == 1
    }

    pub fn can_remove(&self) -> bool {
        ((self.permission >> 7) & 1) == 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SettingItem {
    pub key: String,
    pub value: String,
    pub help: Option<String>,
    pub r#type: String,
    pub options: Option<String>,
    pub group: i32,
    pub flag: i32,
    pub index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Storage {
    pub id: i64,
    pub mount_path: String,
    pub order: i32,
    pub driver: String,
    pub cache_expiration: i32,
    pub status: Option<String>,
    pub addition: Option<String>,
    pub remark: Option<String>,
    pub disabled: bool,
    pub enable_sign: bool,
    pub order_by: Option<String>,
    pub order_direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Meta {
    pub id: i64,
    pub path: String,
    pub password: Option<String>,
    pub p_sub: bool,
    pub hide: Option<String>,
    pub h_sub: bool,
    pub readme: Option<String>,
    pub r_sub: bool,
    pub header: Option<String>,
    pub header_sub: bool,
}
