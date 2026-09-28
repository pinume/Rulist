import {
  Modal,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalOverlay,
  ModalCloseButton,
  createDisclosure,
} from "@hope-ui/solid"
import { lazy, onCleanup, Show, Suspense } from "solid-js"
import { Portal } from "solid-js/web"
import { FullLoading } from "~/components"
import { bus } from "~/utils"
import { BackTop } from "./BackTop"
import { BatchRename } from "./BatchRename"
import { Copy, Move } from "./CopyMove"
import { Delete } from "./Delete"
import { PackageDownloadModal } from "./Download"
import { Mkdir } from "./Mkdir"
import { Rename } from "./Rename"

const Upload = lazy(() => import("../uploads/Upload"))

const UploadModal = () => {
  const { isOpen, onOpen, onClose } = createDisclosure()
  const handler = (name: string) => {
    if (name === "upload") onOpen()
  }
  bus.on("tool", handler)
  onCleanup(() => bus.off("tool", handler))
  return (
    <Modal
      opened={isOpen()}
      onClose={onClose}
      closeOnOverlayClick={false}
      closeOnEsc={false}
      size={{
        "@initial": "xs",
        "@md": "md",
        "@lg": "lg",
        "@xl": "xl",
        "@2xl": "2xl",
      }}
    >
      <ModalOverlay />
      <ModalContent>
        <ModalCloseButton />
        <ModalHeader>Upload</ModalHeader>
        <ModalBody>
          <Show when={isOpen()}>
            <Suspense fallback={<FullLoading />}>
              <Upload />
            </Suspense>
          </Show>
        </ModalBody>
      </ModalContent>
    </Modal>
  )
}

export const Modals = () => (
  <>
    <Copy />
    <Move />
    <Rename />
    <Delete />
    <Mkdir />
    <BatchRename />
    <PackageDownloadModal />
    <UploadModal />
  </>
)

export const Toolbar = () => (
  <Portal>
    <Modals />
    <BackTop />
  </Portal>
)
