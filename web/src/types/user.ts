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
  disabled: boolean
  password_unset: boolean
  // otp: boolean;
}

export const UserPermissionBits = {
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
    return (
      UserMethods.is_admin(user) ||
      ((user.permission >> permission) & 1) === 1
    )
  },
}
