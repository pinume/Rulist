export enum UserRole {
  GENERAL = 0,
  ADMIN = 2,
}

export interface SessionUser {
  id: number
  username: string
  role: UserRole
  permission: number
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

export const isAdmin = (user: SessionUser) => user.role === UserRole.ADMIN

export const can = (user: SessionUser, permission: number) =>
  isAdmin(user) || ((user.permission >> permission) & 1) === 1
