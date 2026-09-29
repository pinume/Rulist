import { notify } from "~/utils"

export const useUtil = () => {
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
  }
}
