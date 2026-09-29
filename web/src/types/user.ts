export enum UserRole {
  GENERAL = 0,
  ADMIN = 2,
}

export interface User {
  id: number
  username: string
  local_path: string
  role: UserRole
  permission: number
  disabled: boolean
  password_unset: boolean
  otp: boolean
}

export const UserPermissionBits = {
  write_content: 3,
  rename: 4,
  move: 5,
  copy: 6,
  delete: 7,
  overwrite: 8,
  allow_empty_password: 9,
} as const

export const UserPermissions = Object.keys(UserPermissionBits) as Array<
  keyof typeof UserPermissionBits
>

export const isAdmin = (user: User) => user.role === UserRole.ADMIN

export const can = (user: User, permission: number) =>
  isAdmin(user) || ((user.permission >> permission) & 1) === 1
