import { Box, createDisclosure, VStack } from "@hope-ui/solid"
import { createMemo, Show } from "solid-js"
import { RightIcon } from "./Icon"
import { CgMoreO } from "solid-icons/cg"
import { TbCheckbox } from "solid-icons/tb"
import { objStore, selectAll, State, toggleCheckbox, userCan } from "~/store"
import { bus } from "~/utils"
import { operations } from "./operations"
import { AiOutlineCloudUpload, AiOutlineSetting } from "solid-icons/ai"
import { RiSystemRefreshLine } from "solid-icons/ri"
import { usePath, useRouter } from "~/hooks"

export const Right = () => {
  const { isOpen, onToggle } = createDisclosure({
    defaultIsOpen: localStorage.getItem("more-open") === "true",
    onClose: () => localStorage.setItem("more-open", "false"),
    onOpen: () => localStorage.setItem("more-open", "true"),
  })
  const margin = createMemo(() => (isOpen() ? "$4" : "$5"))
  const isFolder = createMemo(() => objStore.state === State.Folder)
  const { refresh } = usePath()
  const { to } = useRouter()
  return (
    <Box
      class="left-toolbar-box"
      pos="fixed"
      right={margin()}
      bottom={margin()}
      zIndex="calc($modal - 1)"
    >
      <Show
        when={isOpen()}
        fallback={
          <RightIcon
            class="toolbar-toggle"
            as={CgMoreO}
            onClick={() => {
              onToggle()
            }}
          />
        }
      >
        <VStack
          class="left-toolbar"
          p="$1"
          rounded="$lg"
          spacing="$1"
          bgColor="$neutral1"
        >
          <VStack spacing="$1" class="left-toolbar-in">
            <Show
              when={
                isFolder() &&
                (userCan("write_content") || objStore.write_content_bypass) &&
                objStore.write
              }
            >
              <RightIcon
                as={RiSystemRefreshLine}
                tips="refresh"
                onClick={() => {
                  refresh(undefined, true)
                }}
              />
              <RightIcon
                as={operations.new_file.icon}
                tips="new_file"
                onClick={() => {
                  bus.emit("tool", "new_file")
                }}
              />
              <RightIcon
                as={operations.mkdir.icon}
                p="$1_5"
                tips="mkdir"
                onClick={() => {
                  bus.emit("tool", "mkdir")
                }}
              />
            </Show>
            <Show when={isFolder() && userCan("move") && objStore.write}>
              <RightIcon
                as={operations.recursive_move.icon}
                tips="recursive_move"
                onClick={() => {
                  bus.emit("tool", "recursiveMove")
                }}
              />
            </Show>
            <Show when={isFolder() && userCan("delete") && objStore.write}>
              <RightIcon
                as={operations.remove_empty_directory.icon}
                tips="remove_empty_directory"
                onClick={() => {
                  bus.emit("tool", "removeEmptyDirectory")
                }}
              />
            </Show>
            <Show when={isFolder() && userCan("rename") && objStore.write}>
              <RightIcon
                as={operations.batch_rename.icon}
                tips="batch_rename"
                onClick={() => {
                  selectAll(true)
                  bus.emit("tool", "batchRename")
                }}
              />
            </Show>
            <Show
              when={
                isFolder() &&
                (userCan("write_content") || objStore.write_content_bypass) &&
                objStore.write
              }
            >
              <RightIcon
                as={AiOutlineCloudUpload}
                tips="upload"
                onClick={() => {
                  bus.emit("tool", "upload")
                }}
              />
            </Show>
            <RightIcon
              tips="toggle_checkbox"
              as={TbCheckbox}
              onClick={toggleCheckbox}
            />
            <RightIcon
              as={AiOutlineSetting}
              tips="settings"
              onClick={() => {
                to("/@settings")
              }}
            />
          </VStack>
          <RightIcon tips="more" as={CgMoreO} onClick={onToggle} />
        </VStack>
      </Show>
    </Box>
  )
}
