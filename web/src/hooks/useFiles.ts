import axios, { Canceler } from "axios"
import {
  FileStore,
  LIST_PAGE_SIZE,
  OrderBy,
  ViewState,
  loadSortState,
  getHistoryKey,
  hasHistory,
  recoverHistory,
  clearHistory,
  shouldKeepState,
  fileStore,
} from "~/store"
import { fsGet, fsList, handleRespWithoutNotify, pathJoin } from "~/utils"
import { useFetch } from "./useFetch"
import { useRouter } from "./useRouter"

let cancelFile: Canceler
let cancelList: Canceler

const directoryPaths: Record<string, boolean> = {}
export const useFiles = () => {
  const { pathname, to } = useRouter()
  const [, getFile] = useFetch((path: string) =>
    fsGet(
      path,
      new axios.CancelToken((c) => {
        cancelFile = c
      }),
    ),
  )
  const [, listFiles] = useFetch(
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
        arg?.orderBy ?? fileStore.orderBy,
        arg?.reverse ?? fileStore.reverse,
      )
    },
  )
  // set a path must be a dir
  const rememberDirectory = (path: string, dir = true, push = false) => {
    if (push) {
      path = pathJoin(pathname(), path)
    }
    if (dir) {
      directoryPaths[path] = true
    } else {
      delete directoryPaths[path]
    }
  }

  // load a pathname
  // if confirm current path is dir, fetch List directly
  // if not, fetch get then determine if it is dir or file
  const loadPath = (
    path: string,
    force?: boolean,
    page = 1,
  ) => {
    cancelFile?.()
    cancelList?.()
    FileStore.setErr("")
    const { orderBy, reverse } = loadSortState(path)
    FileStore.setSort(orderBy, reverse)
    if (hasHistory(path)) {
      console.log(`handle [${getHistoryKey(path)}] from history`)
      return recoverHistory(path)
    } else if (directoryPaths[path]) {
      console.log(`handle [${getHistoryKey(path)}] as folder`)
      return loadFolder(path, force, page)
    } else {
      console.log(`handle [${getHistoryKey(path)}] as file`)
      return loadFile(path)
    }
  }

  // Load a path whose type is not known yet.
  const loadFile = async (path: string) => {
    shouldKeepState() || FileStore.setState(ViewState.Loading)
    const resp = await getFile(path)
    handleRespWithoutNotify(
      resp,
      (data) => {
        FileStore.setFile(data)
        if (data.is_dir) {
          rememberDirectory(path)
          loadFolder(path)
        } else {
          FileStore.setRawUrl(data.raw_url)
          shouldKeepState() || FileStore.setState(ViewState.File)
        }
      },
      handleErr,
    )
  }

  // enter a folder
  const loadFolder = async (
    path: string,
    force?: boolean,
    page = 1,
    orderBy = fileStore.orderBy,
    reverse = fileStore.reverse,
  ) => {
    shouldKeepState() || FileStore.setState(ViewState.Loading)
    const resp = await listFiles({ path, force, page, orderBy, reverse })
    handleRespWithoutNotify(
      resp,
      (data) => {
        const lastPage = Math.max(1, Math.ceil(data.total / LIST_PAGE_SIZE))
        if (page > lastPage) {
          void loadFolder(path, force, lastPage, orderBy, reverse)
          return
        }
        FileStore.setListing(data.content ?? [], data.total, page)
        FileStore.setWrite(data.write)
        shouldKeepState() || FileStore.setState(ViewState.Folder)
      },
      handleErr,
    )
  }

  const handleErr = (msg: string, code?: number) => {
    if (code === undefined || code >= 0) {
      FileStore.setErr(msg)
    }
  }
  return {
    loadPath,
    loadFolder,
    rememberDirectory,
    refresh: async (force?: boolean) => {
      const path = pathname()
      const scroll = window.scrollY
      clearHistory(path)
      await loadPath(path, force, fileStore.page)
      window.scroll({ top: scroll, behavior: "smooth" })
    },
  }
}
