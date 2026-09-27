import { createEffect } from "solid-js"
import { getSetting } from "~/store"
import { pathBase } from "~/utils"
import { useRouter } from "./useRouter"
import { useT } from "./useT"

export const useTitle = (title: string | (() => string)) => {
  if (typeof title === "function") {
    createEffect(() => {
      document.title = title()
    })
  } else {
    document.title = title
  }
}

export const useObjTitle = () => {
  const t = useT()
  const { pathname } = useRouter()
  useTitle(
    () =>
      `${
        pathname() === "/" ? t("global.home") : pathBase(pathname())
      } | ${getSetting("site_title")}`,
  )
}
