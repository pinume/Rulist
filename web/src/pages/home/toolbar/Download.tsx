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
import { useT } from "~/hooks"
import { getSettingBool } from "~/store"
import { bus, notify } from "~/utils"
import { CenterIcon } from "./Icon"

export const Download = () => (
  <CenterIcon
    name="download"
    onClick={() => {
      bus.emit("tool", "package_download_direct")
    }}
  />
)

const PackageDownload = lazy(() => import("./PackageDownload"))

export const PackageDownloadModal = () => {
  const t = useT()
  const handler = (name: string) => {
    if (name === "package_download" || name === "package_download_direct") {
      if (!getSettingBool("package_download")) {
        notify.warning(t("home.toolbar.package_download_disabled"))
        return
      }
      setShow(
        name === "package_download_direct" ? "package_download" : "pre_tips",
      )
      onOpen()
    }
  }
  bus.on("tool", handler)
  onCleanup(() => {
    bus.off("tool", handler)
  })
  const { isOpen, onOpen, onClose } = createDisclosure()
  const [show, setShow] = createSignal("pre_tips")
  return (
    <Modal
      blockScrollOnMount={false}
      opened={isOpen()}
      onClose={onClose}
      closeOnOverlayClick={false}
      closeOnEsc={false}
      // size={{
      //   "@initial": "xs",
      //   "@md": "md",
      // }}
    >
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>{t("home.toolbar.package_download")}</ModalHeader>
        <Suspense fallback={<FullLoading />}>
          <Show
            when={show() === "pre_tips"}
            fallback={<PackageDownload onClose={onClose} />}
          >
            <ModalBody>
              <p>{t("home.toolbar.pre_package_download-tips")}</p>
            </ModalBody>
            <ModalFooter display="flex" gap="$2">
              <Button onClick={onClose} colorScheme="neutral">
                {t("global.cancel")}
              </Button>
              <Button
                colorScheme="info"
                onClick={() => {
                  setShow("package_download")
                }}
              >
                {t("global.confirm")}
              </Button>
            </ModalFooter>
          </Show>
        </Suspense>
      </ModalContent>
    </Modal>
  )
}
