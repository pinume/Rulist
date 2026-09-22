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
  password = "",
  cancelToken?: CancelToken,
): Promise<FsGetResp> => {
  return r.post(
    "/fs/get",
    {
      path: path,
      password: password,
    },
    {
      cancelToken: cancelToken,
    },
  )
}
export const fsList = (
  path: string = "/",
  password = "",
  page = 1,
  per_page = 0,
  refresh = false,
  cancelToken?: CancelToken,
): Promise<FsListResp> => {
  return r.post(
    "/fs/list",
    {
      path,
      password,
      page,
      per_page,
      refresh,
    },
    {
      cancelToken: cancelToken,
    },
  )
}

export const fsDirs = (
  path = "/",
  password = "",
  forceRoot = false,
): PResp<Obj[]> => {
  return r.post("/fs/dirs", { path, password, force_root: forceRoot })
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
  overwrite?: boolean,
  skip_existing?: boolean,
  conflict_policy?: ConflictPolicy,
): PEmptyResp => {
  const policy: ConflictPolicy =
    conflict_policy ??
    (overwrite ? "overwrite" : skip_existing ? "skip" : "cancel")
  return r.post("/fs/move", {
    src_dir,
    dst_dir,
    names,
    overwrite: policy === "overwrite",
    skip_existing: policy === "skip",
    conflict_policy: policy,
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
  overwrite?: boolean,
  skip_existing?: boolean,
  merge?: boolean,
  conflict_policy?: ConflictPolicy,
): PEmptyResp => {
  const policy: ConflictPolicy =
    conflict_policy ??
    (overwrite ? "overwrite" : skip_existing ? "skip" : "cancel")
  return r.post("/fs/copy", {
    src_dir,
    dst_dir,
    names,
    overwrite: policy === "overwrite",
    skip_existing: policy === "skip",
    merge,
    conflict_policy: policy,
  })
}

export const fsRemove = (dir: string, names: string[]): PEmptyResp => {
  return r.post("/fs/remove", { dir, names })
}

export const fsRemoveEmptyDirectory = (src_dir: string): PEmptyResp => {
  return r.post("/fs/remove_empty_directory", { src_dir })
}

export const fsNewFile = (
  path: string,
  password: string,
  overwrite: boolean,
): PEmptyResp => {
  return r.put("/fs/put", undefined, {
    headers: {
      "File-Path": encodeURIComponent(path),
      Password: password,
      Overwrite: overwrite.toString(),
    },
  })
}
