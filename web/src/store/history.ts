import {
  shouldKeepState,
  FileStore,
  fileStore,
  ViewState,
  getFileRequestGeneration,
} from "~/store/files"
import { encodePath } from "~/utils/path"

interface History {
  state: object
  scroll: number
}

export const HistoryMap = new Map<string, History>()
const HISTORY_LIMIT = 50

const waitForNextFrame = () => {
  return new Promise((resolve) => setTimeout(resolve))
}

export const getHistoryKey = (path: string) => encodePath(path)

export const recordHistory = (path: string) => {
  try {
    if (![ViewState.Folder, ViewState.File].includes(fileStore.state)) {
      return
    }
    const state = JSON.parse(JSON.stringify(fileStore))
    const key = getHistoryKey(path)
    const history = {
      state,
      scroll: typeof window !== "undefined" ? window.scrollY : 0,
    }
    HistoryMap.delete(key)
    HistoryMap.set(key, history)
    if (HistoryMap.size > HISTORY_LIMIT) {
      HistoryMap.delete(HistoryMap.keys().next().value!)
    }
  } catch (err) {
    console.warn("failed to record history:", err)
  }
}

export const recoverHistory = async (path: string) => {
  try {
    const key = getHistoryKey(path)
    const history = HistoryMap.get(key)
    if (!history) return
    const generation = getFileRequestGeneration()
    shouldKeepState() || FileStore.setState(ViewState.Initial)
    await waitForNextFrame()
    if (generation !== getFileRequestGeneration() || HistoryMap.get(key) !== history) return
    HistoryMap.delete(key)
    HistoryMap.set(key, history)
    FileStore.set(JSON.parse(JSON.stringify(history.state)))
    await waitForNextFrame()
    if (generation !== getFileRequestGeneration() || HistoryMap.get(key) !== history) return
    if (typeof window !== "undefined") {
      window.scroll({ top: history.scroll })
    }
  } catch (err) {
    console.warn("failed to recover history:", err)
  }
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
