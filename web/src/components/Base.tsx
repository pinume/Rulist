import {
  Box,
  Center,
  Flex,
  Heading,
  useColorModeValue,
} from "@hope-ui/solid"
import { mergeProps, Show, JSXElement } from "solid-js"

export const Error = (props: {
  msg: string
  h?: string
  actions?: JSXElement
}) => {
  const merged = mergeProps(
    {
      h: "$full",
    },
    props,
  )
  return (
    <Center h={merged.h} p="$2" flexDirection="column">
      <Box
        rounded="$lg"
        px="$4"
        py="$6"
        bgColor={useColorModeValue("white", "$neutral3")()}
      >
        <Heading
          css={{
            wordBreak: "break-all",
          }}
        >
          {props.msg}
        </Heading>
        <Show when={props.actions}>
          <Flex mt="$4" justifyContent="center">
            {props.actions}
          </Flex>
        </Show>
      </Box>
    </Center>
  )
}
