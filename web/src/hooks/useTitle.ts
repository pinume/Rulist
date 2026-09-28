import { createEffect } from "solid-js"
import { getSetting } from "~/store"
import { pathBase } from "~/utils"
import { useRouter } from "./useRouter"

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
  const { pathname } = useRouter()
  useTitle(
    () =>
      `${pathname() === "/" ? "Home" : pathBase(pathname())} | ${getSetting("site_title")}`,
  )
}
