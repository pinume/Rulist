import { createMemo, createSignal } from "solid-js"
import { createStore } from "solid-js/store"
import { FileEntry, FileItem, FileType } from "~/types"

export type OrderBy = "name" | "size" | "modified"
export const LIST_PAGE_SIZE = 100
type SortState = { orderBy: OrderBy; reverse: boolean }
const defaultSort: SortState = { orderBy: "name", reverse: false }
let fileRequestGeneration = 0

export const getFileRequestGeneration = () => fileRequestGeneration
export const invalidateFileRequests = () => {
  fileRequestGeneration++
}

export const saveSortState = (dir: string, state: SortState) => {
  try {
    localStorage.setItem(`dir_sort_${dir}`, JSON.stringify(state))
  } catch (err) {
    console.warn("failed to save sort config:", err)
  }
}

export const loadSortState = (dir: string): SortState => {
  try {
    const item = localStorage.getItem(`dir_sort_${dir}`)
    if (!item) return defaultSort
    const state = JSON.parse(item) as SortState
    if (
      ["name", "size", "modified"].includes(state.orderBy) &&
      typeof state.reverse === "boolean"
    ) {
      return state
    }
  } catch (err) {
    console.warn("failed to read sort config:", err)
  }
  return defaultSort
}

export enum ViewState {
  Initial,
  Loading,
  Folder,
  File,
}
const createInitialFileStore = () => ({
  file: {} as FileEntry,
  raw_url: "",
  files: [] as FileItem[],
  total: 0,
  page: 1,
  orderBy: "name" as OrderBy,
  reverse: false,
  state: ViewState.Initial,
  err: "",
})
const [fileStore, setFileStore] = createStore<
  ReturnType<typeof createInitialFileStore>
>(createInitialFileStore())

const setListing = (files: FileEntry[], total: number, page: number) => {
  if (fileStore.page !== page) setDirectoryFilter("")
  setFileStore({ files, total, page })
  setFileStore("file", "is_dir", true)
}

export const FileStore = {
  set: (data: object) => setFileStore(data),
  setFile: (file: FileEntry) => setFileStore("file", file),
  setRawUrl: (raw_url: string) => setFileStore("raw_url", raw_url),
  setListing,
  setSort: (orderBy: OrderBy, reverse: boolean) => setFileStore({ orderBy, reverse }),
  setState: (state: ViewState) => setFileStore("state", state),
  setErr: (err: string) => setFileStore("err", err),
}

let lastClickedIndex: number | null = null

export const setLastClickedIndex = (index: number | null) => {
  lastClickedIndex = index
}

const directoryPaths: Record<string, boolean> = {}
export const rememberDirectoryPath = (path: string, dir: boolean) => {
  if (dir) directoryPaths[path] = true
  else delete directoryPaths[path]
}
export const isKnownDirectoryPath = (path: string) =>
  directoryPaths[path] === true

export const selectRange = (targetIndex: number) => {
  const indexes = visibleFileIndexes()
  if (!indexes.includes(targetIndex)) return
  if (lastClickedIndex === null || !indexes.includes(lastClickedIndex)) {
    selectIndex(targetIndex, true)
    lastClickedIndex = targetIndex
    return
  }
  const posA = indexes.indexOf(lastClickedIndex)
  const posB = indexes.indexOf(targetIndex)
  const start = Math.min(posA, posB)
  const end = Math.max(posA, posB)
  for (let i = start; i <= end; i++) {
    setFileStore("files", indexes[i], { selected: true })
  }
}

export const selectIndex = (index: number, checked: boolean, one?: boolean) => {
  const indexes = visibleFileIndexes()
  if (!indexes.includes(index)) return
  if (one) selectAll(false)
  setFileStore("files", index, { selected: checked })
}

export const selectAll = (checked: boolean) => {
  if (!checked) {
    lastClickedIndex = null
  }
  const indexes = checked
    ? visibleFileIndexes()
    : fileStore.files.map((_, index) => index)
  for (const index of indexes) setFileStore("files", index, { selected: checked })
}

export const selectedFiles = () => fileStore.files.filter((file) => file.selected)
export const oneSelected = () => selectedNum() === 1

const selectedNum = createMemo(() => selectedFiles().length)
export { fileStore }
const [directoryFilter, setDirectoryFilterValue] = createSignal("")
export const setDirectoryFilter = (value: string) => {
  if (directoryFilter() === value) return
  selectAll(false)
  setDirectoryFilterValue(value)
}
export const clearDirectoryFilter = () => setDirectoryFilter("")
export { directoryFilter }
export const visibleFileIndexes = createMemo(() => {
  const query = directoryFilter().trim().toLowerCase()
  const indexes = fileStore.files.flatMap((obj, index) =>
    !query || obj.name.toLowerCase().includes(query) ? [index] : [],
  )
  const orderBy = fileStore.orderBy
  const reverse = fileStore.reverse

  return indexes.sort((i, j) => {
    const a = fileStore.files[i]
    const b = fileStore.files[j]
    if (!a || !b) return 0
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
    let res = 0
    if (orderBy === "size") {
      res = a.size - b.size
    } else if (orderBy === "modified") {
      res = (new Date(a.modified).getTime() || 0) - (new Date(b.modified).getTime() || 0)
    } else {
      res = a.name.localeCompare(b.name, undefined, { numeric: true })
    }
    if (res === 0) res = a.name.localeCompare(b.name, undefined, { numeric: true })
    const orderedRes = reverse ? -res : res
    if (orderedRes !== 0) return orderedRes
    return 0
  })
})
const getCountStr = (objs: FileItem[], prefix: "count" | "selected", filterType?: FileType) => {
  if (filterType) objs = objs.filter((obj) => obj.is_dir || obj.type === filterType)
  if (objs.length === 0) return ""
  const folders = objs.filter((o) => o.is_dir).length
  const files = objs.length - folders
  const label = prefix === "count" ? "This page" : "Selected"
  if (folders && files) return `${label}: ${folders} folders, ${files} files`
  if (folders) return `${label}: ${folders} folders`
  return `${label}: ${files} files`
}

export const countMsg = (filterType?: FileType) =>
  getCountStr(fileStore.files, "count", filterType)

export const selectedMsg = (filterType?: FileType) => {
  const selectedList = selectedFiles()
  return selectedList.length > 0
    ? getCountStr(selectedList, "selected", filterType)
    : ""
}

export const resetFileState = () => {
  invalidateFileRequests()
  setFileStore(createInitialFileStore())
  setDirectoryFilterValue("")
  lastClickedIndex = null
  for (const path of Object.keys(directoryPaths)) delete directoryPaths[path]
  setUploadConfig({ asTask: false, overwrite: false })
  setShouldKeepState(false)
}

export const [uploadConfig, setUploadConfig] = createStore({
  asTask: false,
  overwrite: false,
})

export const [shouldKeepState, setShouldKeepState] = createSignal(false)
