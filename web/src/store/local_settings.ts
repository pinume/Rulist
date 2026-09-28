import { createSignal } from "solid-js"

const [sortPos, setSortPos] = createSignal(
  localStorage.getItem("folder_sort_position") || "top",
)

export const local = {
  get folder_sort_position() {
    return sortPos()
  },
}

export const setLocal = (_key: string, val: string) => {
  localStorage.setItem("folder_sort_position", val)
  setSortPos(val)
}
