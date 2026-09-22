export enum UserRole {
  GENERAL = 0,
  ADMIN = 2,
}

export interface User {
  id: number
  username: string
  password: string
  base_path: string
  local_path: string
  role: UserRole
  permission: number
  sso_id: string
  disabled: boolean
  // otp: boolean;
}

export const UserPermissionBits = {
  see_hides: 0,
  access_without_password: 1,
  write_content: 3,
  rename: 4,
  move: 5,
  copy: 6,
  delete: 7,
} as const

export const UserPermissions = Object.keys(UserPermissionBits) as Array<
  keyof typeof UserPermissionBits
>

export const UserMethods = {
  is_admin: (user: User) => user.role === UserRole.ADMIN,
  is_general: (user: User) => user.role === UserRole.GENERAL,
  can: (user: User, permission: number) => {
    return ((user.permission >> permission) & 1) == 1
  },
  // can_see_hides: (user: User) =>
  //   UserMethods.is_admin(user) || (user.permission & 1) == 1,
  // can_access_without_password: (user: User) =>
  //   UserMethods.is_admin(user) || ((user.permission >> 1) & 1) == 1,
  // can_offline_download_tasks: (user: User) =>
  //   UserMethods.is_admin(user) || ((user.permission >> 2) & 1) == 1,
  // can_write: (user: User) =>
  //   UserMethods.is_admin(user) || ((user.permission >> 3) & 1) == 1,
  // can_rename: (user: User) =>
  //   UserMethods.is_admin(user) || ((user.permission >> 4) & 1) == 1,
  // can_move: (user: User) =>
  //   UserMethods.is_admin(user) || ((user.permission >> 5) & 1) == 1,
  // can_copy: (user: User) =>
  //   UserMethods.is_admin(user) || ((user.permission >> 6) & 1) == 1,
  // can_remove: (user: User) =>
  //   UserMethods.is_admin(user) || ((user.permission >> 7) & 1) == 1,
}
