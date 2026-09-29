import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  Button,
  createDisclosure,
} from "@hope-ui/solid"
import { onCleanup } from "solid-js"
import { useFetch, usePath, useRouter } from "~/hooks"
import { selectedFiles } from "~/store"
import { bus, fsRemove, handleRespWithNotifySuccess } from "~/utils"

export const Delete = () => {
  const { isOpen, onOpen, onClose } = createDisclosure()
  const [loading, ok] = useFetch(fsRemove)
  const { refresh } = usePath()
  const { pathname } = useRouter()
  const handler = (name: string) => {
    if (name === "delete") onOpen()
  }
  bus.on("tool", handler)
  onCleanup(() => bus.off("tool", handler))
  return (
    <Modal
      blockScrollOnMount={false}
      opened={isOpen()}
      onClose={onClose}
      size={{ "@initial": "xs", "@md": "md" }}
    >
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>Delete</ModalHeader>
        <ModalBody><p>Delete the selected items?</p></ModalBody>
        <ModalFooter display="flex" gap="$2">
          <Button onClick={onClose} colorScheme="neutral">Cancel</Button>
          <Button
            colorScheme="danger"
            loading={loading()}
            onClick={async () => {
              const resp = await ok(pathname(), selectedFiles().map((obj) => obj.name))
              refresh()
              handleRespWithNotifySuccess(resp, onClose)
            }}
          >
            Confirm
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
