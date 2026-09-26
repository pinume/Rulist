import { createMemo, createSignal } from "solid-js"
import { createStore } from "solid-js/store"
import { Obj, ObjType, StoreObj } from "~/types"
import { useT } from "~/hooks"
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
  Initial, // Initial state
  FetchingObj,
  FetchingObjs,
  Folder, // Folder state
  File, // File state
  NeedPassword,
}
const initialObjStore = {
  obj: {} as Obj,
  raw_url: "",
  related: [] as Obj[],

  objs: [] as StoreObj[],

  readme: "",
  header: "",
  provider: "",
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
    write_content_bypass?: boolean
  }
>(initialObjStore)

const setListing = (objs: Obj[], total: number, page: number) => {
  if (objStore.page !== page) {
    setDirectoryFilter("")
  }
  setObjStore({ objs, total, page })
  setObjStore("obj", "is_dir", true)
}

export const ObjStore = {
  set: (data: object) => {
    setObjStore(data)
  },
  setObj: (obj: Obj) => {
    setObjStore("obj", obj)
  },
  setRawUrl: (raw_url: string) => {
    setObjStore("raw_url", raw_url)
  },
  setProvider: (provider: string) => {
    setObjStore("provider", provider)
  },
  setListing: setListing,
  setSort: (orderBy: OrderBy, reverse: boolean) =>
    setObjStore({ orderBy, reverse }),
  setReadme: (readme: string) => setObjStore("readme", readme),
  setHeader: (header: string) => setObjStore("header", header),
  setRelated: (related: Obj[]) => setObjStore("related", related),
  setWrite: (write: boolean) => setObjStore("write", write),
  setWriteContentBypass: (write_content_bypass: boolean) =>
    setObjStore("write_content_bypass", write_content_bypass),
  setState: (state: State) => setObjStore("state", state),
  setErr: (err: string) => setObjStore("err", err),
}

export const selectIndex = (index: number, checked: boolean, one?: boolean) => {
  const indexes = visibleObjIndexes()
  if (!indexes.includes(index)) return
  if (one) {
    selectAll(false)
  }
  setObjStore("objs", index, { selected: checked })
}

export const selectAll = (checked: boolean) => {
  const indexes = checked
    ? visibleObjIndexes()
    : objStore.objs.map((_, index) => index)
  for (const index of indexes) {
    setObjStore("objs", index, { selected: checked })
  }
}

export const selectedObjs = () => {
  return objStore.objs.filter((obj) => obj.selected)
}

export const allChecked = () => {
  const indexes = visibleObjIndexes()
  return (
    indexes.length > 0 &&
    indexes.every((index) => objStore.objs[index].selected)
  )
}

export const oneChecked = () => {
  return selectedNum() === 1
}

export const haveSelected = () => {
  return selectedNum() > 0
}

export const isIndeterminate = () => {
  const selected = visibleObjIndexes().filter(
    (index) => objStore.objs[index].selected,
  )
  return selected.length > 0 && selected.length < visibleObjIndexes().length
}

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
    if (position === "top" && a.is_dir !== b.is_dir) {
      return a.is_dir ? -1 : 1
    }
    let res = 0
    if (orderBy === "size") {
      res = a.size - b.size
    } else if (orderBy === "modified") {
      const aTime = new Date(a.modified).getTime() || 0
      const bTime = new Date(b.modified).getTime() || 0
      res = aTime - bTime
    } else {
      res = a.name.localeCompare(b.name, undefined, { numeric: true })
    }
    if (res === 0) {
      res = a.name.localeCompare(b.name, undefined, { numeric: true })
    }
    const orderedRes = reverse ? -res : res
    if (orderedRes !== 0) return orderedRes
    if (a.is_dir !== b.is_dir) {
      return a.is_dir ? -1 : 1
    }
    return 0
  })
})
const [password, setPassword] = createSignal<string>("")
export { password, setPassword }

const getCountStr = (
  objs: StoreObj[],
  prefix: string,
  filterType?: ObjType,
) => {
  const t = useT()

  if (filterType) {
    objs = objs.filter((obj) => obj.is_dir || obj.type === filterType)
  }

  if (objs.length === 0) return ""

  const folders = objs.filter((o) => o.is_dir).length
  const files = objs.length - folders
  const vars = { folders: folders.toString(), files: files.toString() }
  const key =
    folders && files
      ? `${prefix}`
      : folders
        ? `${prefix}_folders`
        : files
          ? `${prefix}_files`
          : ""
  return key ? t(`home.obj.count.${key}`, vars) : ""
}

export const countMsg = (filterType?: ObjType) =>
  getCountStr(objStore.objs, "count", filterType)

export const selectedMsg = (filterType?: ObjType) => {
  const selectedList = selectedObjs()
  const isSelected = selectedList.length > 0

  return isSelected ? getCountStr(selectedList, "selected", filterType) : ""
}

export const [uploadConfig, setUploadConfig] = createStore({
  asTask: false,
  overwrite: false,
})

export const [shouldKeepState, setShouldKeepState] = createSignal(false)
