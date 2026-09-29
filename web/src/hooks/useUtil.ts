import { getHideFiles } from "~/store"
import { FileEntry } from "~/types"
import { notify, pathJoin } from "~/utils"
import { useRouter } from "."

export const useUtil = () => {
  const { pathname } = useRouter()
  return {
    copy: async (text: string) => {
      try {
        await navigator.clipboard.writeText(text)
        notify.success("Copied")
      } catch {
        notify.error(
          "Clipboard access was denied. Allow clipboard permission in your browser settings.",
        )
      }
    },
    isHide: (obj: FileEntry) => {
      const fullPath = pathJoin(pathname(), obj.name)
      return getHideFiles().some((reg) => reg.test(fullPath))
    },
    isHidePath: (path: string) => getHideFiles().some((reg) => reg.test(path)),
  }
}
