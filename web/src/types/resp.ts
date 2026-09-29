import { FileEntry } from "."

export interface Resp<T> {
  code: number
  message: string
  data: T
}

export type FsListResp = Resp<{
  content: FileEntry[]
  total: number
}>

export type FsGetResp = Resp<FileEntry & { raw_url: string }>

export type EmptyResp = Resp<{}>

export type PResp<T> = Promise<Resp<T>>
export type PEmptyResp = Promise<EmptyResp>
