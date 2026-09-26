import { Portal } from "solid-js/web"
import { Copy, Move } from "./CopyMove"
import { Delete } from "./Delete"
import { Rename } from "./Rename"
import { Mkdir } from "./Mkdir"
import { BatchRename } from "./BatchRename"
import { PackageDownloadModal } from "./Download"
import { lazy } from "solid-js"
import { ModalWrapper } from "./ModalWrapper"
import { BackTop } from "./BackTop"

const Upload = lazy(() => import("../uploads/Upload"))

export const Modal = () => {
  return (
    <>
      <Copy />
      <Move />
      <Rename />
      <Delete />
      <Mkdir />
      <BatchRename />
      <PackageDownloadModal />
      <ModalWrapper name="upload" title="home.toolbar.upload">
        <Upload />
      </ModalWrapper>
    </>
  )
}

export const Toolbar = () => {
  return (
    <Portal>
      <Modal />
      <BackTop />
    </Portal>
  )
}
