import { getHideFiles } from "~/store"
import { Obj } from "~/types"
import { notify, pathJoin } from "~/utils"
import { useT, useRouter } from "."

export const useUtil = () => {
  const t = useT()
  const { pathname } = useRouter()
  return {
    copy: async (text: string) => {
      let copied = false
      try {
        await navigator.clipboard.writeText(text)
        copied = true
      } catch {
        const ta = document.createElement("textarea")
        ta.value = text
        ta.style.position = "fixed"
        ta.style.opacity = "0"
        const root = document.fullscreenElement ?? document.body
        try {
          root.appendChild(ta)
          ta.select()
          copied = document.execCommand("copy")
        } catch {
          copied = false
        } finally {
          ta.remove()
        }
      }
      if (copied) {
        notify.success(t("global.copied"))
      } else {
        notify.error(t("global.clipboard_denied"))
      }
    },
    isHide: (obj: Obj) => {
      const fullPath = pathJoin(pathname(), obj.name)
      return getHideFiles().some((reg) => reg.test(fullPath))
    },
    isHidePath: (path: string) => {
      return getHideFiles().some((reg) => reg.test(path))
    },
  }
}
