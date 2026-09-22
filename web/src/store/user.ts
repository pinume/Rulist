import { createSignal } from "solid-js"
import {
  User,
  UserMethods,
  UserPermissionBits,
  UserPermissions,
} from "~/types"

export type Me = User & { otp: boolean }
const [me, setMe] = createSignal<Me>({} as Me)

type Permission = (typeof UserPermissions)[number]
export const userCan = (p: Permission) => {
  const u = me()
  return UserMethods.can(u, UserPermissionBits[p])
}

export { me, setMe }
