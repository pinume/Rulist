import { useColorModeValue } from "@hope-ui/solid"

export const hoverColor = () => "rgba(132,133,141,0.18)"

export const alphaBgColor = () =>
  useColorModeValue("$whiteAlpha10", "$blackAlpha11")()
