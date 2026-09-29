import { Obj } from "."

export interface Resp<T> {
  code: number
  message: string
  data: T
}

export type FsListResp = Resp<{
  content: Obj[]
  total: number
  write: boolean
}>

export type FsGetResp = Resp<Obj & { raw_url: string }>

export type EmptyResp = Resp<{}>

export type PResp<T> = Promise<Resp<T>>
export type PEmptyResp = Promise<EmptyResp>
