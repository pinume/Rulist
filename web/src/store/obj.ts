import { createMemo, createSignal } from "solid-js"
import { createStore } from "solid-js/store"
import { Obj, ObjType, StoreObj } from "~/types"
import { local } from "./local_settings"

export type OrderBy = "name" | "size" | "modified"
export const LIST_PAGE_SIZE = 100
type SortState = { orderBy: OrderBy; reverse: boolean }
const defaultSort: SortState = { orderBy: "name", reverse: false }

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

export enum State {
  Initial,
  FetchingObj,
  FetchingObjs,
  Folder,
  File,
}
const initialObjStore = {
  obj: {} as Obj,
  raw_url: "",
  objs: [] as StoreObj[],
  total: 0,
  page: 1,
  orderBy: "name" as OrderBy,
  reverse: false,
  state: State.Initial,
  err: "",
}
const [objStore, setObjStore] = createStore<
  typeof initialObjStore & {
    write?: boolean
  }
>(initialObjStore)

const setListing = (objs: Obj[], total: number, page: number) => {
  if (objStore.page !== page) setDirectoryFilter("")
  setObjStore({ objs, total, page })
  setObjStore("obj", "is_dir", true)
}

export const ObjStore = {
  set: (data: object) => setObjStore(data),
  setObj: (obj: Obj) => setObjStore("obj", obj),
  setRawUrl: (raw_url: string) => setObjStore("raw_url", raw_url),
  setListing,
  setSort: (orderBy: OrderBy, reverse: boolean) => setObjStore({ orderBy, reverse }),
  setWrite: (write: boolean) => setObjStore("write", write),
  setState: (state: State) => setObjStore("state", state),
  setErr: (err: string) => setObjStore("err", err),
}

let lastClickedIndex: number | null = null

export const setLastClickedIndex = (index: number | null) => {
  lastClickedIndex = index
}

export const selectRange = (targetIndex: number) => {
  const indexes = visibleObjIndexes()
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
    setObjStore("objs", indexes[i], { selected: true })
  }
}

export const selectIndex = (index: number, checked: boolean, one?: boolean) => {
  const indexes = visibleObjIndexes()
  if (!indexes.includes(index)) return
  if (one) selectAll(false)
  setObjStore("objs", index, { selected: checked })
}

export const selectAll = (checked: boolean) => {
  if (!checked) {
    lastClickedIndex = null
  }
  const indexes = checked
    ? visibleObjIndexes()
    : objStore.objs.map((_, index) => index)
  for (const index of indexes) setObjStore("objs", index, { selected: checked })
}

export const selectedObjs = () => objStore.objs.filter((obj) => obj.selected)
export const oneChecked = () => selectedNum() === 1

const selectedNum = createMemo(() => selectedObjs().length)
export { objStore }
const [directoryFilter, setDirectoryFilterValue] = createSignal("")
export const setDirectoryFilter = (value: string) => {
  if (directoryFilter() === value) return
  selectAll(false)
  setDirectoryFilterValue(value)
}
export const clearDirectoryFilter = () => setDirectoryFilter("")
export { directoryFilter }
export const visibleObjIndexes = createMemo(() => {
  const query = directoryFilter().trim().toLowerCase()
  const indexes = objStore.objs.flatMap((obj, index) =>
    !query || obj.name.toLowerCase().includes(query) ? [index] : [],
  )
  const position = (local["folder_sort_position"] || "top") as string
  const orderBy = objStore.orderBy
  const reverse = objStore.reverse

  return indexes.sort((i, j) => {
    const a = objStore.objs[i]
    const b = objStore.objs[j]
    if (!a || !b) return 0
    if (position === "top" && a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
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
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
    return 0
  })
})
const getCountStr = (objs: StoreObj[], prefix: "count" | "selected", filterType?: ObjType) => {
  if (filterType) objs = objs.filter((obj) => obj.is_dir || obj.type === filterType)
  if (objs.length === 0) return ""
  const folders = objs.filter((o) => o.is_dir).length
  const files = objs.length - folders
  const label = prefix === "count" ? "This page" : "Selected"
  if (folders && files) return `${label}: ${folders} folders, ${files} files`
  if (folders) return `${label}: ${folders} folders`
  return `${label}: ${files} files`
}

export const countMsg = (filterType?: ObjType) =>
  getCountStr(objStore.objs, "count", filterType)

export const selectedMsg = (filterType?: ObjType) => {
  const selectedList = selectedObjs()
  return selectedList.length > 0
    ? getCountStr(selectedList, "selected", filterType)
    : ""
}

export const [uploadConfig, setUploadConfig] = createStore({
  asTask: false,
  overwrite: false,
})

export const [shouldKeepState, setShouldKeepState] = createSignal(false)
