export const startDownload = (rawUrl: string, name: string) => {
  const url = new URL(rawUrl, window.location.href)
  url.searchParams.set("openlist_ts", Date.now().toString())
  const anchor = document.createElement("a")
  anchor.href = url.toString()
  anchor.download = name
  anchor.click()
}
