import { fsLink, fsList } from "./api"
import { api } from "./request"
import { pathJoin } from "./path"
import { FileEntry } from "~/types"

export const collectDownloadFiles = async (
  root: string,
  selected: FileEntry[],
): Promise<{ path: string }[]> => {
  const pending = selected.map((file) => ({ path: file.name, is_dir: file.is_dir })).reverse()
  const files: { path: string }[] = []
  // ponytail: one directory request at a time; use a bounded worker pool if enumeration is slow.
  while (pending.length > 0) {
    const entry = pending.pop()!
    if (!entry.is_dir) {
      files.push({ path: entry.path })
      continue
    }
    const resp = await fsList(pathJoin(root, entry.path))
    if (resp.code !== 200) throw new Error(resp.message)
    for (const item of [...(resp.data.content ?? [])].reverse()) {
      pending.push({ path: pathJoin(entry.path, item.name), is_dir: item.is_dir })
    }
  }
  return files
}

export const startDownload = (rawUrl: string, name: string) => {
  const anchor = document.createElement("a")
  anchor.href = rawUrl
  anchor.download = name
  anchor.click()
}

export const getFreshDownloadUrl = async (path: string): Promise<string> => {
  const resp = await fsLink(path)
  if (resp.code !== 200) throw new Error(resp.message)
  const raw = resp.data.url
  return raw.startsWith("http://") || raw.startsWith("https://")
    ? raw
    : `${api}${raw}`
}

export const downloadPath = async (path: string, name: string) => {
  startDownload(await getFreshDownloadUrl(path), name)
}

export const openPath = async (path: string) => {
  const tab = window.open("about:blank", "_blank")
  if (!tab) throw new Error("Unable to open a new tab")
  tab.opener = null
  try {
    tab.location.href = await getFreshDownloadUrl(path)
  } catch (error) {
    tab.close()
    throw error
  }
}
