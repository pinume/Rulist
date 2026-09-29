import { createSignal } from "solid-js"
import {
  can as hasPermission,
  User,
  UserPermissionBits,
  UserPermissions,
} from "~/types"

const [currentUser, setCurrentUser] = createSignal<User | null>(null)

type Permission = (typeof UserPermissions)[number]
export const can = (permission: Permission) => {
  const user = currentUser()
  return user !== null && hasPermission(user, UserPermissionBits[permission])
}

export { currentUser, setCurrentUser }
