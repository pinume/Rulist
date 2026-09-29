import { createMemo, createSignal } from "solid-js"
import { PublicConfig } from "~/types"

export const [config, setConfig] = createSignal<PublicConfig | null>(null)
export const loadConfig = (value: PublicConfig) => {
  setConfig(value)
  console.log(
    `%c Rulist %c ${value.version} %c https://github.com/pinume/Rulist`,
    "color: #fff; background: #5f5f5f",
    "color: #fff; background: #70c6be",
    "",
  )
}

export const logos = () => {
  const logo = config()?.logo.trim()
  if (!logo || logo === "favicon.ico") {
    return ["rulist.svg", "rulist-dark.svg"] as const
  }
  const [light, dark] = logo.split(/\r?\n/).map((item) => item.trim())
  return [light || "rulist.svg", dark || light || "rulist-dark.svg"] as const
}

export const mainColor = () => config()?.main_color || "#1890ff"

export const hideFilePatterns = createMemo(() =>
  (config()?.hide_files ?? []).flatMap((item) => {
    const match = item.match(/^\/(.*)\/([a-z]*)$/)
    try {
      return [new RegExp(match?.[1] ?? item, match?.[2] ?? "")]
    } catch (error) {
      console.warn("[config] invalid regex in hide_files:", item, error)
      return []
    }
  }),
)
