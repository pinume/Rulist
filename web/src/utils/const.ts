import { useColorModeValue } from "@hope-ui/solid"

export const hoverColor = () => "rgba(132,133,141,0.18)"

export const alphaBgColor = () =>
  useColorModeValue("$whiteAlpha10", "$blackAlpha11")()

export function colorAlpha(color: string, alpha: number): string {
  if (!color) return color
  return `color-mix(in srgb, ${color} ${Math.round(alpha * 100)}%, transparent)`
}
