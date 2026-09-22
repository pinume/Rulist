import { shouldKeepState, ObjStore, objStore, State } from "~/store/obj"
import { encodePath } from "~/utils"

interface History {
  obj: object
  scroll: number
}

export const HistoryMap = new Map<string, History>()

const waitForNextFrame = () => {
  return new Promise((resolve) => setTimeout(resolve))
}

export const getHistoryKey = (path: string) => encodePath(path)

export const recordHistory = (path: string) => {
  if (![State.Folder, State.File].includes(objStore.state)) {
    return
  }
  const obj = JSON.parse(JSON.stringify(objStore))
  const key = getHistoryKey(path)
  const history = {
    obj,
    scroll: window.scrollY,
  }
  HistoryMap.set(key, history)
  console.log(`record history: [${key}]`)
}

export const recoverHistory = async (path: string) => {
  const key = getHistoryKey(path)
  const history = HistoryMap.get(key)
  if (!history) return
  shouldKeepState() || ObjStore.setState(State.Initial)
  await waitForNextFrame()
  ObjStore.set(JSON.parse(JSON.stringify(history.obj)))
  await waitForNextFrame()
  window.scroll({ top: history.scroll })
}

export const hasHistory = (path: string) => {
  const key = getHistoryKey(path)
  return HistoryMap.has(key)
}

export const clearHistory = (path: string) => {
  const key = getHistoryKey(path)
  if (hasHistory(path)) {
    HistoryMap.delete(key)
    console.log(`clear history: [${key}]`)
  }
}

document.addEventListener(
  "click",
  (e) => {
    let target = e.target as HTMLElement
    let link = target.closest("a")
    let path = link?.getAttribute("href")
    if (path && path.startsWith("/")) {
      clearHistory(decodeURIComponent(path))
    }
  },
  true,
)
