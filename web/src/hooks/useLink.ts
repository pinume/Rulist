import { me, objStore, State } from "~/store"
import { FileEntry } from "~/types"
import { api, encodePath, pathDir, pathJoin, standardizePath } from "~/utils"
import { useRouter } from "."

// get download url by dir and obj
export const getLinkByDirAndObj = (
  dir: string,
  obj: FileEntry,
  encodeAll?: boolean,
) => {
  dir = standardizePath(dir, true)
  const path = encodePath(`${dir}/${obj.name}`, encodeAll)
  let ans = `${api}/d${path}`
  if (obj.sign) {
    ans += `?sign=${obj.sign}&uid=${me().id}`
  }
  return ans
}

// get download link by current state and pathname
export const useLink = () => {
  const { pathname } = useRouter()
  const rawLink = (obj: FileEntry, encodeAll?: boolean) => {
    const dir = objStore.state === State.File ? pathDir(pathname()) : pathname()
    return getLinkByDirAndObj(dir, obj, encodeAll)
  }
  return { rawLink }
}
