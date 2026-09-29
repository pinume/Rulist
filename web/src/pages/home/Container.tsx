import { Box } from "@hope-ui/solid"
import { JSXElement } from "solid-js"

export const Container = (props: { children: JSXElement }) => (
  <Box w="$full">{props.children}</Box>
)
