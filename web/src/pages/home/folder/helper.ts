import { createEffect, onCleanup } from "solid-js"
import { objStore, selectedObjs, selectIndex } from "~/store"
import { StoreObj } from "~/types"

let selectedCache: StoreObj[] | null = null

export function useSelectWithMouse() {
  const isMouseSupported = () => false
  const openWithDoubleClick = () => false
  const toggleWithClick = () => false

  const saveSelectionCache = () => {
    selectedCache = selectedObjs()
  }

  const restoreSelectionCache = () => {
    if (selectedCache === null) return false
    for (let i = 0; i < objStore.objs.length; ++i) {
      selectIndex(i, selectedCache.indexOf(objStore.objs[i]) >= 0)
    }
    return true
  }

  const registerSelectContainer = () => {
    createEffect(() => {
      const area = document.querySelector(".viselect-container")
      area?.addEventListener("mousedown", saveSelectionCache)
      onCleanup(() =>
        area?.removeEventListener("mousedown", saveSelectionCache),
      )
    })
  }

  return {
    isMouseSupported,
    openWithDoubleClick,
    toggleWithClick,
    restoreSelectionCache,
    registerSelectContainer,
  }
}
