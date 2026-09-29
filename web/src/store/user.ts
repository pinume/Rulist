import { createSignal } from "solid-js"
import { User, UserMethods, UserPermissionBits, UserPermissions } from "~/types"

const [me, setMe] = createSignal<User>({} as User)

type Permission = (typeof UserPermissions)[number]
export const userCan = (p: Permission) => {
  const u = me()
  return UserMethods.can(u, UserPermissionBits[p])
}

export { me, setMe, UserMethods }
