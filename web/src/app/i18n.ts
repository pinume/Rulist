import * as i18n from "@solid-primitives/i18n"
import { createSignal } from "solid-js"
import { dict as rawDictionary } from "~/lang/zh-CN/entry"
export { i18n }

export type RawDictionary = typeof rawDictionary
export type Dictionary = i18n.Flatten<RawDictionary>

const dictionary: Dictionary = i18n.flatten(rawDictionary)

export const [currentLang] = createSignal("zh-CN")
export const dict = () => dictionary
