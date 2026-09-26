import { bus } from "~/utils"
import { UploadFileProps } from "./types"

export const traverseFileTree = async (entry: FileSystemEntry) => {
  const res: File[] = []

  const internalProcess = async (entry: FileSystemEntry, path: string) => {
    await new Promise<void>((resolve, reject) => {
      const errorCallback: ErrorCallback = (e) => {
        console.error(e)
        reject(e)
      }
      if (entry.isFile) {
        ;(entry as FileSystemFileEntry).file((file) => {
          const newFile = new File([file], path + file.name, {
            type: file.type,
            lastModified: file.lastModified,
          })
          res.push(newFile)
          resolve()
        }, errorCallback)
      } else if (entry.isDirectory) {
        const dirReader = (entry as FileSystemDirectoryEntry).createReader()
        const readEntries = () => {
          dirReader.readEntries(async (entries) => {
            for (let i = 0; i < entries.length; i++) {
              await internalProcess(entries[i], path + entry.name + "/")
            }
            if (entries.length > 0) {
              readEntries()
            } else {
              resolve()
            }
          }, errorCallback)
        }
        readEntries()
      }
    })
  }
  await internalProcess(entry, "")
  return res
}

export const extractFilesFromDataTransfer = async (
  dataTransfer: DataTransfer | null,
): Promise<File[]> => {
  if (!dataTransfer) return []
  const items = Array.from(dataTransfer.items ?? [])
  const files = Array.from(dataTransfer.files ?? [])

  if (items.length === 0) {
    return files
  }

  const entries: { isDirectory: boolean; entry?: FileSystemEntry; file?: File }[] = []
  for (let i = 0; i < items.length; i++) {
    const item = items[i]
    if (item.kind !== "file") continue
    const entry = item.webkitGetAsEntry?.()
    if (entry?.isDirectory) {
      entries.push({ isDirectory: true, entry })
    } else if (files[i]) {
      entries.push({ isDirectory: false, file: files[i] })
    }
  }

  const res: File[] = []
  for (const item of entries) {
    if (item.isDirectory && item.entry) {
      try {
        const innerFiles = await traverseFileTree(item.entry)
        res.push(...innerFiles)
      } catch (e) {
        console.error("Failed to traverse directory", e)
      }
    } else if (item.file) {
      res.push(item.file)
    }
  }

  return res
}

export const File2Upload = (file: File): UploadFileProps => {
  return {
    name: file.name,
    path: file.webkitRelativePath || file.name,
    size: file.size,
    progress: 0,
    speed: 0,
    status: "pending",
  }
}

let pendingFiles: File[] = []
let uploadListenerActive = false

export const setUploadListenerActive = (active: boolean) => {
  uploadListenerActive = active
}

export const enqueueFilesForUpload = (files: File[]) => {
  if (files.length === 0) return
  if (uploadListenerActive) {
    bus.emit("upload_files", files)
  } else {
    pendingFiles.push(...files)
  }
  bus.emit("tool", "upload")
}

export const takePendingFiles = (): File[] => {
  const files = pendingFiles
  pendingFiles = []
  return files
}
