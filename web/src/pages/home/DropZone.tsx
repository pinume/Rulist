import { Box, Heading, Text, VStack } from "@hope-ui/solid"
import { FiUploadCloud } from "solid-icons/fi"
import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js"
import { Portal } from "solid-js/web"
import { useRouter, useT } from "~/hooks"
import { getMainColor, objStore, State, userCan } from "~/store"
import { notify } from "~/utils"
import { enqueueFilesForUpload, extractFilesFromDataTransfer } from "./uploads/util"

export const DropZone = () => {
  const t = useT()
  const { pathname } = useRouter()
  const [isDragging, setIsDragging] = createSignal(false)
  let dragCounter = 0

  const canWrite = () =>
    objStore.state === State.Folder &&
    (userCan("write_content") || !!objStore.write_content_bypass) &&
    !!objStore.write

  const hasOpenModal = () =>
    !!document.querySelector(".hope-modal__overlay, .hope-modal__content")

  const isFilesDrag = (e: DragEvent) =>
    e.dataTransfer?.types?.includes("Files") ?? false

  const resetDrag = () => {
    dragCounter = 0
    setIsDragging(false)
  }

  createEffect(() => {
    pathname()
    objStore.state
    resetDrag()
  })

  const onDragEnter = (e: DragEvent) => {
    if (!canWrite() || hasOpenModal() || !isFilesDrag(e)) return
    dragCounter++
    setIsDragging(true)
  }

  const onDragOver = (e: DragEvent) => {
    if (!isFilesDrag(e) || hasOpenModal()) return
    e.preventDefault()
    if (canWrite() && e.dataTransfer) {
      e.dataTransfer.dropEffect = "copy"
    }
  }

  const onDragLeave = (e: DragEvent) => {
    if (!canWrite()) return
    dragCounter--
    if (
      dragCounter <= 0 ||
      e.clientX <= 0 ||
      e.clientY <= 0 ||
      e.clientX >= window.innerWidth ||
      e.clientY >= window.innerHeight
    ) {
      resetDrag()
    }
  }

  const onDrop = async (e: DragEvent) => {
    const isFiles = isFilesDrag(e)
    if (isFiles && !hasOpenModal()) {
      e.preventDefault()
    }
    resetDrag()
    if (!canWrite() || hasOpenModal() || !isFiles) return

    const files = await extractFilesFromDataTransfer(e.dataTransfer)
    if (files.length === 0) {
      notify.warning(t("home.upload.no_files_drag"))
      return
    }
    enqueueFilesForUpload(files)
  }

  onMount(() => {
    window.addEventListener("dragenter", onDragEnter)
    window.addEventListener("dragover", onDragOver)
    window.addEventListener("dragleave", onDragLeave)
    window.addEventListener("drop", onDrop)
    window.addEventListener("dragend", resetDrag)
    window.addEventListener("blur", resetDrag)
  })

  onCleanup(() => {
    window.removeEventListener("dragenter", onDragEnter)
    window.removeEventListener("dragover", onDragOver)
    window.removeEventListener("dragleave", onDragLeave)
    window.removeEventListener("drop", onDrop)
    window.removeEventListener("dragend", resetDrag)
    window.removeEventListener("blur", resetDrag)
  })

  return (
    <Portal>
      <Show when={isDragging()}>
        <Box
          position="fixed"
          top="0"
          left="0"
          w="100vw"
          h="100vh"
          zIndex={1400}
          pointerEvents="none"
          display="flex"
          alignItems="center"
          justifyContent="center"
          bg="rgba(0, 0, 0, 0.45)"
          css={{
            backdropFilter: "blur(4px)",
          }}
          p="$6"
        >
          <VStack
            spacing="$3"
            p="$8"
            rounded="$2xl"
            bg="$neutral1"
            border="2px dashed"
            borderColor={getMainColor()}
            shadow="$2xl"
            maxW="90vw"
          >
            <Box color={getMainColor()} fontSize="3.5rem">
              <FiUploadCloud />
            </Box>
            <Heading size="lg">
              {t("home.upload.release_to_upload")}
            </Heading>
            <Text fontSize="$sm" color="$neutral11">
              {pathname()}
            </Text>
          </VStack>
        </Box>
      </Show>
    </Portal>
  )
}
