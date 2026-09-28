CREATE TRIGGER IF NOT EXISTS `x_users_passwordless_insert_invariant`
BEFORE INSERT ON `x_users`
WHEN NEW.`role` != 2
  AND NEW.`password_unset` != 0
  AND (NEW.`permission` & 512) = 0
BEGIN
    SELECT RAISE(ABORT, 'passwordless login permission is required for an empty password');
END;
