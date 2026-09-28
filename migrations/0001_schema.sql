CREATE TABLE IF NOT EXISTS `x_users` (
    `id` INTEGER PRIMARY KEY AUTOINCREMENT,
    `username` TEXT NOT NULL UNIQUE,
    `pwd_hash` TEXT NOT NULL,
    `pwd_ts` INTEGER NOT NULL,
    `base_path` TEXT NOT NULL DEFAULT '/',
    `role` INTEGER NOT NULL DEFAULT 0,
    `disabled` NUMERIC NOT NULL DEFAULT 0,
    `permission` INTEGER NOT NULL DEFAULT 0,
    `password_unset` NUMERIC NOT NULL DEFAULT 0
        CHECK (
            `password_unset` = 0
            OR (
                `role` != 2
                AND (`permission` & 512) != 0
            )
        ),
    `otp_secret` TEXT,
    `last_otp_step` INTEGER NOT NULL DEFAULT -1
);

CREATE TABLE IF NOT EXISTS `x_setting_items` (
    `key` TEXT PRIMARY KEY,
    `value` TEXT NOT NULL,
    `help` TEXT,
    `type` TEXT NOT NULL DEFAULT 'string',
    `options` TEXT,
    `group` INTEGER NOT NULL DEFAULT 0,
    `flag` INTEGER NOT NULL DEFAULT 0,
    `index` INTEGER
);

CREATE TABLE IF NOT EXISTS `x_storages` (
    `id` INTEGER PRIMARY KEY AUTOINCREMENT,
    `mount_path` TEXT NOT NULL UNIQUE,
    `order` INTEGER NOT NULL DEFAULT 0,
    `status` TEXT,
    `addition` TEXT,
    `disabled` NUMERIC NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS `x_login_attempts` (
    `username_hash` TEXT PRIMARY KEY,
    `failed_count` INTEGER NOT NULL,
    `window_started` INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS `x_revoked_tokens` (
    `jti` TEXT PRIMARY KEY,
    `expires_at` INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS `x_users_single_admin`
    ON `x_users` (`role`) WHERE `role` = 2;
