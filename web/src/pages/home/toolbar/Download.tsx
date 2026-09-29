import {
  Button,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
  ModalOverlay,
  createDisclosure,
} from "@hope-ui/solid"
import { createSignal, lazy, onCleanup, Show, Suspense } from "solid-js"
import { FullLoading } from "~/components"
import { config } from "~/store"
import { bus, notify } from "~/utils"

const PackageDownload = lazy(() => import("./PackageDownload"))

export const PackageDownloadModal = () => {
  const handler = (name: string) => {
    if (name === "package_download" || name === "package_download_direct") {
      if (!config()?.package_download) {
        notify.warning("Archive download is disabled")
        return
      }
      setShow(
        name === "package_download_direct" ? "package_download" : "pre_tips",
      )
      onOpen()
    }
  }
  bus.on("tool", handler)
  onCleanup(() => bus.off("tool", handler))
  const { isOpen, onOpen, onClose } = createDisclosure()
  const [show, setShow] = createSignal("pre_tips")
  return (
    <Modal
      blockScrollOnMount={false}
      opened={isOpen()}
      onClose={onClose}
      closeOnOverlayClick={false}
      closeOnEsc={false}
    >
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>Download as archive</ModalHeader>
        <Suspense fallback={<FullLoading />}>
          <Show
            when={show() === "pre_tips"}
            fallback={<PackageDownload onClose={onClose} />}
          >
            <ModalBody>
              <p>
                Browser-based archive downloads use StreamSaver instead of the server and require CORS support from the storage service. Unsupported storage may cause the download to fail.
              </p>
            </ModalBody>
            <ModalFooter display="flex" gap="$2">
              <Button onClick={onClose} colorScheme="neutral">Cancel</Button>
              <Button colorScheme="info" onClick={() => setShow("package_download")}>
                Confirm
              </Button>
            </ModalFooter>
          </Show>
        </Suspense>
      </ModalContent>
    </Modal>
  )
}
