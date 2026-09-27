INSERT OR IGNORE INTO `x_setting_items`
    (`key`, `value`, `type`, `group`, `flag`)
VALUES
    ('permission_overwrite_v1_migrated', 'true', 'bool', 0, 1);

UPDATE `x_users`
SET `permission` = `permission` | (1 << 8)
WHERE `role` != 2
  AND (`permission` & 120) != 0;

UPDATE `x_users`
SET `base_path` = '/.users/' || `id`,
    `disabled` = 1
WHERE `role` != 2
  AND `base_path` = '/';

UPDATE `x_users`
SET `base_path` = '/'
WHERE `role` = 2
  AND `base_path` LIKE '/.users/%';
