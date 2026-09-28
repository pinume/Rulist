CREATE TRIGGER IF NOT EXISTS `x_users_passwordless_invariant`
BEFORE UPDATE OF `permission`, `password_unset` ON `x_users`
WHEN NEW.`role` != 2
  AND NEW.`password_unset` != 0
  AND (NEW.`permission` & 512) = 0
BEGIN
    SELECT RAISE(ABORT, 'set a non-empty password before disabling passwordless login');
END;
