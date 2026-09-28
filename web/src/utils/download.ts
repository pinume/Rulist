export const startDownload = (rawUrl: string, name: string) => {
  const anchor = document.createElement("a")
  anchor.href = rawUrl
  anchor.download = name
  anchor.click()
}
