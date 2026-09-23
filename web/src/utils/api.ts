import { CancelToken } from "axios"
import {
  PEmptyResp,
  FsGetResp,
  FsListResp,
  Obj,
  PResp,
  RenameObj,
} from "~/types"
import { r } from "."

export const fsGet = (
  path: string = "/",
  cancelToken?: CancelToken,
): Promise<FsGetResp> => {
  return r.post(
    "/fs/get",
    {
      path: path,
    },
    {
      cancelToken: cancelToken,
    },
  )
}
export const fsList = (
  path: string = "/",
  page = 1,
  per_page = 0,
  cancelToken?: CancelToken,
  order_by?: "name" | "size" | "modified",
  reverse?: boolean,
): Promise<FsListResp> => {
  return r.post(
    "/fs/list",
    {
      path,
      page,
      per_page,
      order_by,
      reverse,
    },
    {
      cancelToken: cancelToken,
    },
  )
}

export const fsDirs = (path = "/", forceRoot = false): PResp<Obj[]> => {
  return r.post("/fs/dirs", { path, force_root: forceRoot })
}

export const fsMkdir = (path: string): PEmptyResp => {
  return r.post("/fs/mkdir", { path })
}

export const fsRename = (
  path: string,
  name: string,
  overwrite: boolean,
): PEmptyResp => {
  return r.post("/fs/rename", { path, name, overwrite })
}

export const fsBatchRename = (
  src_dir: string,
  rename_objects: RenameObj[],
): PEmptyResp => {
  return r.post("/fs/batch_rename", { src_dir, rename_objects })
}

export type ConflictPolicy = "cancel" | "overwrite" | "skip"

export const fsMove = (
  src_dir: string,
  dst_dir: string,
  names: string[],
  conflict_policy: ConflictPolicy = "cancel",
): PEmptyResp => {
  return r.post("/fs/move", {
    src_dir,
    dst_dir,
    names,
    conflict_policy,
  })
}

export const fsRecursiveMove = (
  src_dir: string,
  dst_dir: string,
  conflict_policy: ConflictPolicy,
): PEmptyResp => {
  return r.post("/fs/recursive_move", {
    src_dir,
    dst_dir,
    conflict_policy,
  })
}

export const fsCopy = (
  src_dir: string,
  dst_dir: string,
  names: string[],
  conflict_policy: ConflictPolicy = "cancel",
): PEmptyResp => {
  return r.post("/fs/copy", {
    src_dir,
    dst_dir,
    names,
    conflict_policy,
  })
}

export const fsRemove = (dir: string, names: string[]): PEmptyResp => {
  return r.post("/fs/remove", { dir, names })
}

export const fsRemoveEmptyDirectory = (src_dir: string): PEmptyResp => {
  return r.post("/fs/remove_empty_directory", { src_dir })
}

export const fsNewFile = (path: string, overwrite: boolean): PEmptyResp => {
  return r.put("/fs/put", undefined, {
    headers: {
      "File-Path": encodeURIComponent(path),
      Overwrite: overwrite.toString(),
    },
  })
}

export const fsLink = (path: string): PResp<{ url: string }> => {
  return r.post("/fs/link", { path })
}
