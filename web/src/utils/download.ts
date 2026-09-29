import { fsLink } from "./api"
import { api } from "./request"

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
