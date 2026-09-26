export enum ObjType {
  UNKNOWN,
  FOLDER,
  // OFFICE,
  VIDEO,
  AUDIO,
  TEXT,
  IMAGE,
}

export interface Obj {
  name: string
  size: number
  is_dir: boolean
  created: string
  modified: string
  sign?: string
  raw_url?: string
  thumb: string
  type: ObjType
  permissions?: string
}

export type StoreObj = Obj & {
  selected?: boolean
}

export type RenameObj = {
  src_name: string
  new_name: string
}
