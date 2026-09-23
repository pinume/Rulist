import { createStorageSignal } from "@solid-primitives/storage"
import { createMemo, createSignal } from "solid-js"
import { createStore, produce } from "solid-js/store"
import { Obj, ObjType, StoreObj } from "~/types"
import { bus, log } from "~/utils"
import { keyPressed } from "./key-event"
import { useT } from "~/hooks"

const collator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
})
const naturalCompare = (a: any, b: any) => {
  if (typeof a === "number" && typeof b === "number") {
    return a - b
  }
  return collator.compare(String(a ?? ""), String(b ?? ""))
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
  state: State.Initial,
  err: "",
}
const [objStore, setObjStore] = createStore<
  typeof initialObjStore & {
    write?: boolean
    write_content_bypass?: boolean
  }
>(initialObjStore)

const setObjs = (objs: Obj[]) => {
  lastChecked.start = -1
  lastChecked.end = -1
  setObjStore("objs", objs)
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
  setObjs: setObjs,
  setReadme: (readme: string) => setObjStore("readme", readme),
  setHeader: (header: string) => setObjStore("header", header),
  setRelated: (related: Obj[]) => setObjStore("related", related),
  setWrite: (write: boolean) => setObjStore("write", write),
  setWriteContentBypass: (write_content_bypass: boolean) =>
    setObjStore("write_content_bypass", write_content_bypass),
  setState: (state: State) => setObjStore("state", state),
  setErr: (err: string) => setObjStore("err", err),
}

export type OrderBy = "name" | "size" | "modified"

export const sortObjs = (orderBy: OrderBy, reverse?: boolean) => {
  log("sort:", orderBy, reverse)
  setObjStore(
    "objs",
    produce((objs) =>
      objs.sort((a, b) => {
        return (reverse ? -1 : 1) * naturalCompare(a[orderBy], b[orderBy])
      }),
    ),
  )
}

const lastChecked = {
  start: -1,
  end: -1,
}

export const selectIndex = (index: number, checked: boolean, one?: boolean) => {
  if (one) {
    selectAll(false)
  }
  if (keyPressed["Shift"]) {
    if (lastChecked.start < 0) {
      for (
        let i = 0;
        i < Math.max(index + 1, objStore.objs.length - index);
        ++i
      ) {
        if (objStore.objs[index - i]?.selected) {
          lastChecked.start = index - i
          lastChecked.end = index - i
          break
        } else if (objStore.objs[index + i]?.selected) {
          lastChecked.start = index + i
          lastChecked.end = index + i
          break
        }
      }
    }
    const countUncheck = Math.abs(lastChecked.end - lastChecked.start)
    const signUncheck = Math.sign(lastChecked.end - lastChecked.start)
    for (let i = 1; i <= countUncheck; ++i) {
      setObjStore("objs", lastChecked.start + signUncheck * i, {
        selected: false,
      })
    }
    const countCheck = Math.abs(index - lastChecked.start)
    const signCheck = Math.sign(index - lastChecked.start)
    for (let i = 0; i <= countCheck; ++i) {
      setObjStore("objs", lastChecked.start + signCheck * i, { selected: true })
    }
    lastChecked.end = index
  } else {
    setObjStore("objs", index, { selected: checked })
    if (checked) {
      lastChecked.start = index
      lastChecked.end = index
    } else {
      lastChecked.end = -1
      lastChecked.start = -1
    }
  }
}

export const selectAll = (checked: boolean) => {
  setObjStore("objs", {}, (obj) => ({ selected: checked }))
}

export const selectedObjs = () => {
  return objStore.objs.filter((obj) => obj.selected)
}

export const allChecked = () => {
  return objStore.objs.length === selectedNum()
}

export const oneChecked = () => {
  return selectedNum() === 1
}

export const haveSelected = () => {
  return selectedNum() > 0
}

export const isIndeterminate = () => {
  return selectedNum() > 0 && selectedNum() < objStore.objs.length
}

const selectedNum = createMemo(() => selectedObjs().length)

export type LayoutType = "list" | "grid"
const [pathname, setPathname] = createSignal<string>(location.pathname)
const layoutRecord: Record<string, LayoutType> = (() => {
  try {
    return JSON.parse(localStorage.getItem("layoutRecord") || "{}")
  } catch (e) {
    return {}
  }
})()

bus.on("pathname", (p) => setPathname(p))
const [_layout, _setLayout] = createSignal<LayoutType>(
  layoutRecord[pathname()] === "grid" ? "grid" : "list",
)
export const layout = () => {
  _setLayout(layoutRecord[pathname()] === "grid" ? "grid" : "list")
  return _layout()
}
export const setLayout = (layout: LayoutType) => {
  layoutRecord[pathname()] = layout
  localStorage.setItem("layoutRecord", JSON.stringify(layoutRecord))
  _setLayout(layout)
}

const [_checkboxOpen, setCheckboxOpen] = createStorageSignal<string>(
  "checkbox-open",
  "false",
)
export const checkboxOpen = () => _checkboxOpen() === "true"

export const toggleCheckbox = () => {
  setCheckboxOpen(checkboxOpen() ? "false" : "true")
}

export { objStore }
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

export const smartCountMsg = (filterType?: ObjType) => {
  const selectedList = selectedObjs()
  const isSelected = selectedList.length > 0

  return isSelected
    ? getCountStr(selectedList, "selected", filterType)
    : countMsg(filterType)
}

export const [uploadConfig, setUploadConfig] = createStore({
  asTask: false,
  overwrite: false,
})

export const [shouldKeepState, setShouldKeepState] = createSignal(false)
