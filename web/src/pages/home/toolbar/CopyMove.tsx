import { Checkbox, createDisclosure, VStack, Button } from "@hope-ui/solid"
import { createSignal, onCleanup } from "solid-js"
import { ModalFolderChoose, FolderTreeHandler } from "~/components"
import { useFetch, usePath, useRouter } from "~/hooks"
import { selectedFiles, can } from "~/store"
import {
  bus,
  ConflictPolicy,
  fsCopy,
  fsMove,
  handleRespWithNotifySuccess,
} from "~/utils"
import { CgFolderAdd } from "solid-icons/cg"

export const CreateFolderButton = (props: { handler?: FolderTreeHandler }) => {
  if (!can("write_content")) return null
  return (
    <Button leftIcon={<CgFolderAdd />} size="sm" onClick={() => props.handler?.startCreateFolder()}>
      New folder
    </Button>
  )
}

const CopyMoveModal = (props: { action: "copy" | "move" }) => {
  const { isOpen, onOpen, onClose } = createDisclosure()
  const [loading, ok] = useFetch(props.action === "copy" ? fsCopy : fsMove)
  const { pathname } = useRouter()
  const { refresh } = usePath()
  const [overwrite, setOverwrite] = createSignal(false)
  const [skipExisting, setSkipExisting] = createSignal(false)

  const handler = (name: string) => {
    if (name === props.action) {
      onOpen()
      setOverwrite(false)
      setSkipExisting(false)
    }
  }
  bus.on("tool", handler)
  onCleanup(() => bus.off("tool", handler))

  return (
    <ModalFolderChoose
      header="Select destination folder"
      opened={isOpen()}
      onClose={onClose}
      loading={loading()}
      headerSlot={(handler) => <CreateFolderButton handler={handler} />}
      footerSlot={
        <VStack w="$full" spacing="$2">
          <Checkbox
            mr="auto"
            checked={overwrite()}
            onChange={() => {
              const next = !overwrite()
              if (next) setSkipExisting(false)
              setOverwrite(next)
            }}
          >
            Overwrite existing files
          </Checkbox>
          <Checkbox
            mr="auto"
            checked={skipExisting()}
            onChange={() => setSkipExisting(!skipExisting())}
            disabled={overwrite()}
          >
            Skip existing files
          </Checkbox>
        </VStack>
      }
      onSubmit={async (dst) => {
        const policy: ConflictPolicy = overwrite()
          ? "overwrite"
          : skipExisting()
            ? "skip"
            : "cancel"
        const resp = await ok(
          pathname(),
          dst,
          selectedFiles().map((obj) => obj.name),
          policy,
        )
        refresh()
        handleRespWithNotifySuccess(resp, onClose)
      }}
    />
  )
}

export const Copy = () => <CopyMoveModal action="copy" />
export const Move = () => <CopyMoveModal action="move" />
