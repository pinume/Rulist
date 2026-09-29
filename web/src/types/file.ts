export enum FileType {
  UNKNOWN,
  FOLDER,
  // OFFICE,
  VIDEO,
  AUDIO,
  TEXT,
  IMAGE,
}

export interface FileEntry {
  name: string
  size: number
  is_dir: boolean
  modified: string
  sign?: string
  raw_url?: string
  type: FileType
  permissions?: string
}

export type FileItem = FileEntry & {
  selected?: boolean
}

export type RenameEntry = {
  src_name: string
  new_name: string
}
