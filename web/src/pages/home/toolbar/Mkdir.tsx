import { createDisclosure } from "@hope-ui/solid"
import { ModalInput } from "~/components"
import { useFetch, useFiles, useRouter } from "~/hooks"
import { bus, fsMkdir, handleRespWithNotifySuccess, pathJoin } from "~/utils"
import { onCleanup } from "solid-js"

export const Mkdir = () => {
  const { isOpen, onOpen, onClose } = createDisclosure()
  const [loading, ok] = useFetch(fsMkdir)
  const { pathname } = useRouter()
  const { refresh } = useFiles()
  const handler = (name: string) => {
    if (name === "mkdir") onOpen()
  }
  bus.on("tool", handler)
  onCleanup(() => bus.off("tool", handler))
  return (
    <ModalInput
      title="Enter folder name"
      validateFilename={true}
      opened={isOpen()}
      onClose={onClose}
      loading={loading()}
      onSubmit={async (name) => {
        const resp = await ok(pathJoin(pathname(), name))
        handleRespWithNotifySuccess(resp, () => {
          refresh()
          onClose()
        })
      }}
    />
  )
}
