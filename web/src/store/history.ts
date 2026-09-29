import { shouldKeepState, FileStore, fileStore, ViewState } from "~/store/files"
import { encodePath } from "~/utils/path"

interface History {
  state: object
  scroll: number
}

export const HistoryMap = new Map<string, History>()

const waitForNextFrame = () => {
  return new Promise((resolve) => setTimeout(resolve))
}

export const getHistoryKey = (path: string) => encodePath(path)

export const recordHistory = (path: string) => {
  if (![ViewState.Folder, ViewState.File].includes(fileStore.state)) {
    return
  }
  const state = JSON.parse(JSON.stringify(fileStore))
  const key = getHistoryKey(path)
  const history = {
    state,
    scroll: window.scrollY,
  }
  HistoryMap.set(key, history)
}

export const recoverHistory = async (path: string) => {
  const key = getHistoryKey(path)
  const history = HistoryMap.get(key)
  if (!history) return
  shouldKeepState() || FileStore.setState(ViewState.Initial)
  await waitForNextFrame()
  FileStore.set(JSON.parse(JSON.stringify(history.state)))
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
  }
}

export const clearAllHistory = () => HistoryMap.clear()

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
