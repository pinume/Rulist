import { Checkbox, createDisclosure, VStack, Button } from "@hope-ui/solid"
import { createSignal, onCleanup } from "solid-js"
import { ModalFolderChoose, FolderTreeHandler } from "~/components"
import { useFetch, usePath, useRouter, useT } from "~/hooks"
import { selectedObjs, userCan } from "~/store"
import {
  bus,
  ConflictPolicy,
  fsCopy,
  fsMove,
  handleRespWithNotifySuccess,
} from "~/utils"
import { CgFolderAdd } from "solid-icons/cg"

export const CreateFolderButton = (props: { handler?: FolderTreeHandler }) => {
  if (!userCan("write_content")) {
    return null
  }
  const t = useT()
  return (
    <Button
      leftIcon={<CgFolderAdd />}
      size="sm"
      onClick={() => props.handler?.startCreateFolder()}
    >
      {t("home.toolbar.mkdir")}
    </Button>
  )
}

const CopyMoveModal = (props: { action: "copy" | "move" }) => {
  const t = useT()
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
  onCleanup(() => {
    bus.off("tool", handler)
  })

  return (
    <ModalFolderChoose
      header={t("home.toolbar.choose_dst_folder")}
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
              const curOverwrite = !overwrite()
              if (curOverwrite) {
                setSkipExisting(false)
              }
              setOverwrite(curOverwrite)
            }}
          >
            {t("home.conflict_policy.overwrite_existing")}
          </Checkbox>
          <Checkbox
            mr="auto"
            checked={skipExisting()}
            onChange={() => {
              setSkipExisting(!skipExisting())
            }}
            disabled={overwrite()}
          >
            {t("home.conflict_policy.skip_existing")}
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
          selectedObjs().map((obj) => obj.name),
          policy,
        )
        handleRespWithNotifySuccess(resp, () => {
          refresh()
          onClose()
        })
      }}
    />
  )
}

export const Copy = () => <CopyMoveModal action="copy" />
export const Move = () => <CopyMoveModal action="move" />
