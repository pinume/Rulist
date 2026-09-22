import { Box, HStack, useColorModeValue } from "@hope-ui/solid"
import { createMemo, For, Show } from "solid-js"
import {
  checkboxOpen,
  haveSelected,
  objStore,
  selectAll,
  State,
  userCan,
} from "~/store"
import { CenterIcon } from "./Icon"
import { bus } from "~/utils"
import { Download } from "./Download"

export const Center = () => {
  const show = createMemo(
    () => objStore.state === State.Folder && checkboxOpen() && haveSelected(),
  )
  return (
    <Show when={show()}>
      <Box
        class="center-toolbar"
        pos="fixed"
        bottom="$4"
        left="50%"
        w="max-content"
        color="$neutral11"
        transform="translateX(-50%)"
      >
        <HStack
          p="$2"
          bgColor={useColorModeValue("white", "#000000d0")()}
          spacing="$1"
          shadow="0px 10px 30px -5px rgba(0, 0, 0, 0.3)"
          rounded="$lg"
          css={{
            backdropFilter: "blur(8px)",
          }}
        >
          <Show when={objStore.write}>
            <For each={["rename", "move", "copy", "delete"] as const}>
              {(name) => {
                return userCan(name) ? (
                  <CenterIcon
                    name={name}
                    onClick={() => {
                      bus.emit("tool", name)
                    }}
                  />
                ) : null
              }}
            </For>
          </Show>
          <Download />
          <CenterIcon
            name="cancel_select"
            onClick={() => {
              selectAll(false)
            }}
          />
        </HStack>
      </Box>
    </Show>
  )
}
