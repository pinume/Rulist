import axios, { Canceler } from "axios"
import {
  ObjStore,
  LIST_PAGE_SIZE,
  OrderBy,
  State,
  loadSortState,
  getHistoryKey,
  hasHistory,
  recoverHistory,
  clearHistory,
  me,
  shouldKeepState,
  objStore,
} from "~/store"
import {
  fsGet,
  fsList,
  handleRespWithoutNotify,
  log,
  notify,
  pathJoin,
} from "~/utils"
import { useFetch } from "./useFetch"
import { useRouter } from "./useRouter"

let first_fetch = true

let cancelObj: Canceler
let cancelList: Canceler

const IsDirRecord: Record<string, boolean> = {}
export const usePath = () => {
  const { pathname, to } = useRouter()
  const [, getObj] = useFetch((path: string) =>
    fsGet(
      path,
      new axios.CancelToken((c) => {
        cancelObj = c
      }),
    ),
  )
  const [, getObjs] = useFetch(
    (arg?: {
      path: string
      force?: boolean
      page?: number
      orderBy?: OrderBy
      reverse?: boolean
    }) => {
      return fsList(
        arg?.path,
        arg?.page ?? 1,
        LIST_PAGE_SIZE,
        new axios.CancelToken((c) => {
          cancelList = c
        }),
        arg?.orderBy ?? objStore.orderBy,
        arg?.reverse ?? objStore.reverse,
      )
    },
  )
  // set a path must be a dir
  const setPathAs = (path: string, dir = true, push = false) => {
    if (push) {
      path = pathJoin(pathname(), path)
    }
    if (dir) {
      IsDirRecord[path] = true
    } else {
      delete IsDirRecord[path]
    }
  }

  // record is second time password is wrong
  let retry_pass = false
  // handle pathname change
  // if confirm current path is dir, fetch List directly
  // if not, fetch get then determine if it is dir or file
  const handlePathChange = (
    path: string,
    rp?: boolean,
    force?: boolean,
    page = 1,
  ) => {
    cancelObj?.()
    cancelList?.()
    retry_pass = rp ?? false
    ObjStore.setErr("")
    const { orderBy, reverse } = loadSortState(path)
    ObjStore.setSort(orderBy, reverse)
    if (hasHistory(path)) {
      log(`handle [${getHistoryKey(path)}] from history`)
      return recoverHistory(path)
    } else if (IsDirRecord[path]) {
      log(`handle [${getHistoryKey(path)}] as folder`)
      return handleFolder(path, force, page)
    } else {
      log(`handle [${getHistoryKey(path)}] as obj`)
      return handleObj(path)
    }
  }

  // handle enter obj that don't know if it is dir or file
  const handleObj = async (path: string) => {
    shouldKeepState() || ObjStore.setState(State.FetchingObj)
    const resp = await getObj(path)
    handleRespWithoutNotify(
      resp,
      (data) => {
        ObjStore.setObj(data)
        ObjStore.setProvider(data.provider)
        if (data.is_dir) {
          setPathAs(path)
          handleFolder(path)
        } else {
          ObjStore.setReadme(data.readme)
          ObjStore.setHeader(data.header)
          ObjStore.setRelated(data.related ?? [])
          ObjStore.setRawUrl(data.raw_url)
          shouldKeepState() || ObjStore.setState(State.File)
        }
      },
      handleErr,
    )
  }

  // enter a folder
  const handleFolder = async (
    path: string,
    force?: boolean,
    page = 1,
    orderBy = objStore.orderBy,
    reverse = objStore.reverse,
  ) => {
    shouldKeepState() || ObjStore.setState(State.FetchingObjs)
    const resp = await getObjs({ path, force, page, orderBy, reverse })
    handleRespWithoutNotify(
      resp,
      (data) => {
        const lastPage = Math.max(1, Math.ceil(data.total / LIST_PAGE_SIZE))
        if (page > lastPage) {
          void handleFolder(path, force, lastPage, orderBy, reverse)
          return
        }
        ObjStore.setListing(data.content ?? [], data.total, page)
        ObjStore.setReadme(data.readme)
        ObjStore.setHeader(data.header)
        ObjStore.setWrite(data.write)
        ObjStore.setWriteContentBypass(data.write_content_bypass)
        ObjStore.setProvider(data.provider)
        shouldKeepState() || ObjStore.setState(State.Folder)
      },
      handleErr,
    )
  }

  const handleErr = (msg: string, code?: number) => {
    if (code === 403) {
      ObjStore.setState(State.NeedPassword)
      if (retry_pass) {
        notify.error(msg)
      }
    } else {
      const basePath = me().base_path
      if (
        first_fetch &&
        basePath != "/" &&
        pathname().includes(basePath) &&
        msg.endsWith("object not found")
      ) {
        first_fetch = false
        to(pathname().replace(basePath, ""))
        return
      }
      if (code === undefined || code >= 0) {
        ObjStore.setErr(msg)
      }
    }
  }
  return {
    handlePathChange,
    handleFolder,
    setPathAs,
    refresh: async (retry_pass?: boolean, force?: boolean) => {
      const path = pathname()
      const scroll = window.scrollY
      clearHistory(path)
      await handlePathChange(path, retry_pass, force, objStore.page)
      window.scroll({ top: scroll, behavior: "smooth" })
    },
  }
}
