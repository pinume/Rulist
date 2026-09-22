const settings: Record<string, string> = {}

export const setSettings = (items: Record<string, string>) => {
  Object.keys(items).forEach((key) => {
    settings[key] = items[key]
  })
  const version = settings["version"] || "Unknown"
  console.log(
    `%c Rulist %c ${version} %c https://github.com/pinume/Rulist`,
    "color: #fff; background: #5f5f5f",
    "color: #fff; background: #70c6be",
    "",
  )
}

export const getSetting = (key: string) => settings[key] ?? ""
export const getSettingBool = (key: string) => {
  const value = getSetting(key)
  return value === "true" || value === "1"
}
export const getSettingNumber = (key: string, defaultV?: number) => {
  const value = getSetting(key)
  if (value) {
    return Number(value)
  }
  return defaultV ?? 0
}
export const getMainColor = (): string => {
  if (window.OPENLIST_CONFIG.main_color) {
    return window.OPENLIST_CONFIG.main_color
  }
  return getSetting("main_color") || "#1890ff"
}

let hideFiles: RegExp[]

export const getHideFiles = () => {
  if (!hideFiles) {
    hideFiles = getSetting("hide_files")
      .split(/\n/g)
      .filter((item) => !!item.trim())
      .map((item) => {
        item = item.trim()
        let str = item.replace(/^\/(.*)\/([a-z]*)$/, "$1")
        let args = item.replace(/^\/(.*)\/([a-z]*)$/, "$2")
        return new RegExp(str, args)
      })
  }
  return hideFiles
}
