import { objStore, selectedObjs, State, me } from "~/store"
import { Obj } from "~/types"
import { api, encodePath, pathDir, pathJoin, standardizePath } from "~/utils"
import { useRouter } from "."

// get download url by dir and obj
export const getLinkByDirAndObj = (
  dir: string,
  obj: Obj,
  encodeAll?: boolean,
) => {
  dir = standardizePath(pathJoin(me().base_path, dir), true)
  const path = encodePath(`${dir}/${obj.name}`, encodeAll)
  let ans = `${api}/d${path}`
  if (obj.sign) {
    ans += `?sign=${obj.sign}`
  }
  return ans
}

// get download link by current state and pathname
export const useLink = () => {
  const { pathname } = useRouter()
  const rawLink = (obj: Obj, encodeAll?: boolean) => {
    const dir = objStore.state === State.File ? pathDir(pathname()) : pathname()
    return getLinkByDirAndObj(dir, obj, encodeAll)
  }
  return { rawLink }
}

export const useSelectedLink = () => {
  const { rawLink: rawUrl } = useLink()
  const rawLinks = (encodeAll?: boolean) => {
    return selectedObjs()
      .filter((obj) => !obj.is_dir)
      .map((obj) => rawUrl(obj, encodeAll))
  }
  return {
    rawLinks: rawLinks,
  }
}
