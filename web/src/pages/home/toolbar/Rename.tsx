import { Checkbox, createDisclosure } from "@hope-ui/solid"
import { createSignal, onCleanup, Show } from "solid-js"
import { ModalInput } from "~/components"
import { useFetch, usePath, useRouter } from "~/hooks"
import { oneSelected, selectedFiles } from "~/store"
import { bus, fsRename, handleRespWithNotifySuccess, pathJoin } from "~/utils"

export const Rename = () => {
  const { isOpen, onOpen, onClose } = createDisclosure()
  const [loading, ok] = useFetch(fsRename)
  const { pathname } = useRouter()
  const { refresh } = usePath()
  const [overwrite, setOverwrite] = createSignal(false)
  const handler = (name: string) => {
    if (name === "rename") {
      if (!oneSelected()) {
        bus.emit("tool", "batchRename")
        return
      }
      onOpen()
      setOverwrite(false)
    }
  }
  bus.on("tool", handler)
  onCleanup(() => bus.off("tool", handler))
  return (
    <Show when={isOpen()}>
      <ModalInput
        title="Enter a new name"
        validateFilename={true}
        footerSlot={
          <Checkbox mr="auto" checked={overwrite()} onChange={() => setOverwrite(!overwrite())}>
            Overwrite existing files
          </Checkbox>
        }
        isRenamingFile={!selectedFiles()[0].is_dir}
        opened={isOpen()}
        onClose={onClose}
        defaultValue={selectedFiles()[0]?.name ?? ""}
        loading={loading()}
        onSubmit={async (name) => {
          const resp = await ok(
            pathJoin(pathname(), selectedFiles()[0].name),
            name,
            overwrite(),
          )
          handleRespWithNotifySuccess(resp, () => {
            refresh()
            onClose()
          })
        }}
      />
    </Show>
  )
}
