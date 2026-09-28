import {
  VStack,
  Input,
  Heading,
  HStack,
  IconButton,
  Checkbox,
  Text,
  Badge,
  Progress,
  ProgressIndicator,
  Button,
  Stack,
} from "@hope-ui/solid"
import { createSignal, For, onCleanup, onMount, Show } from "solid-js"
import { usePath, useRouter } from "~/hooks"
import { getMainColor, uploadConfig, setUploadConfig } from "~/store"
import {
  RiDocumentFolderUploadFill,
  RiDocumentFileUploadFill,
} from "solid-icons/ri"
import { bus, getFileSize, notify, pathJoin } from "~/utils"
import { asyncPool } from "~/utils/async_pool"
import { createStore } from "solid-js/store"
import { UploadFileProps, StatusBadge } from "./types"
import {
  File2Upload,
  extractFilesFromDataTransfer,
  setUploadListenerActive,
  takePendingFiles,
} from "./util"
import { StreamUpload } from "./stream"

const statusText: Record<string, string> = {
  pending: "Pending",
  uploading: "Uploading",
  backending: "Uploading in background",
  success: "Success",
  error: "Error",
}

const UploadFile = (props: UploadFileProps & { onRetry?: () => void }) => (
  <VStack
    w="$full"
    spacing="$1"
    rounded="$lg"
    border="1px solid $neutral7"
    alignItems="start"
    p="$2"
    _hover={{ border: `1px solid ${getMainColor()}` }}
  >
    <Text css={{ wordBreak: "break-all" }}>{props.path}</Text>
    <HStack spacing="$2" w="$full" justifyContent="space-between">
      <HStack spacing="$2">
        <Badge colorScheme={StatusBadge[props.status]}>
          {statusText[props.status] ?? props.status}
        </Badge>
        <Text>{getFileSize(props.speed)}/s</Text>
      </HStack>
      <HStack spacing="$2">
        <Show when={props.status === "error" && props.onRetry}>
          <Button compact size="xs" colorScheme="accent" onClick={() => props.onRetry?.()}>
            Retry
          </Button>
        </Show>
        <Text color="$neutral11">{getFileSize(props.size)}</Text>
      </HStack>
    </HStack>
    <Progress
      w="$full"
      trackColor="$info3"
      rounded="$full"
      value={props.progress}
      size="sm"
    >
      <ProgressIndicator color={getMainColor()} rounded="$md" />
    </Progress>
    <Text color="$danger10">{props.msg}</Text>
  </VStack>
)

type UploadTask = UploadFileProps & { id: number }

const Upload = () => {
  const { pathname } = useRouter()
  const { refresh } = usePath()
  const [drag, setDrag] = createSignal(false)
  const [uploading, setUploading] = createSignal(false)
  const [uploadFiles, setUploadFiles] = createStore<{ uploads: UploadTask[] }>({ uploads: [] })
  const allDone = () =>
    uploadFiles.uploads.every(({ status }) => ["success", "error"].includes(status))
  let fileInput!: HTMLInputElement
  let folderInput!: HTMLInputElement
  const fileMap = new Map<number, File>()
  let nextUploadId = 0

  const handleAddFiles = async (files: File[]) => {
    if (files.length === 0) return
    setUploading(true)
    const uploads = files.map((file) => {
      const upload = { ...File2Upload(file), id: nextUploadId++ }
      fileMap.set(upload.id, file)
      setUploadFiles("uploads", (items) => [...items, upload])
      return { file, upload }
    })
    for await (const ms of asyncPool(3, uploads, ({ file, upload }) =>
      handleFile(upload.id, file),
    )) {
      console.log(ms)
    }
    refresh()
    setTimeout(() => refresh(undefined, true), 5000)
  }

  onMount(() => {
    setUploadListenerActive(true)
    const pending = takePendingFiles()
    if (pending.length > 0) handleAddFiles(pending)
  })

  const onUploadFiles = (files: File[]) => handleAddFiles(files)
  bus.on("upload_files", onUploadFiles)
  onCleanup(() => {
    setUploadListenerActive(false)
    bus.off("upload_files", onUploadFiles)
  })

  const setUpload = (id: number, key: keyof UploadFileProps, value: any) => {
    setUploadFiles("uploads", (upload) => upload.id === id, key, value)
  }

  const retryFile = (id: number) => {
    const file = fileMap.get(id)
    if (!file) return
    setUpload(id, "msg", "")
    setUpload(id, "progress", 0)
    setUpload(id, "speed", 0)
    handleFile(id, file)
  }

  const handleFile = async (id: number, file: File) => {
    const path = file.webkitRelativePath || file.name
    setUpload(id, "status", "uploading")
    const uploadPath = pathJoin(pathname(), path)
    try {
      const err = await StreamUpload(
        uploadPath,
        file,
        (key, value) => setUpload(id, key, value),
        uploadConfig.asTask,
        uploadConfig.overwrite,
      ).catch((err) => err)
      if (!err) {
        setUpload(id, "status", "success")
        setUpload(id, "progress", 100)
      } else {
        setUpload(id, "status", "error")
        setUpload(id, "msg", err.message)
      }
    } catch (e: any) {
      console.error(e)
      setUpload(id, "status", "error")
      setUpload(id, "msg", e.message)
    }
  }

  return (
    <VStack w="$full" pb="$2" spacing="$2">
      <Show
        when={!uploading()}
        fallback={
          <>
            <HStack spacing="$2">
              <Button
                colorScheme="accent"
                onClick={() =>
                  setUploadFiles("uploads", (items) =>
                    items.filter(({ status }) => !["success", "error"].includes(status)),
                  )
                }
              >
                Clear completed
              </Button>
              <Show when={allDone()}>
                <Button onClick={() => setUploading(false)}>Back to upload</Button>
              </Show>
            </HStack>
            <For each={uploadFiles.uploads}>
              {(upload) => <UploadFile {...upload} onRetry={() => retryFile(upload.id)} />}
            </For>
          </>
        }
      >
        <Input
          type="file"
          multiple
          ref={fileInput}
          display="none"
          onChange={(e) => handleAddFiles(Array.from(e.currentTarget.files ?? []))}
        />
        <Input
          type="file"
          multiple
          // @ts-ignore
          webkitdirectory
          ref={folderInput}
          display="none"
          onChange={(e) => handleAddFiles(Array.from(e.currentTarget.files ?? []))}
        />
        <VStack
          w="$full"
          justifyContent="center"
          border={`2px dashed ${drag() ? getMainColor() : "$neutral8"}`}
          rounded="$lg"
          spacing="$4"
          p="$6"
          minH="$56"
          onDragOver={(e: DragEvent) => {
            e.preventDefault()
            setDrag(true)
          }}
          onDragLeave={() => setDrag(false)}
          onDrop={async (e: DragEvent) => {
            e.preventDefault()
            e.stopPropagation()
            setDrag(false)
            const files = await extractFilesFromDataTransfer(e.dataTransfer)
            if (files.length === 0) {
              notify.warning("No files were dragged in.")
              return
            }
            handleAddFiles(files)
          }}
        >
          <Show when={!drag()} fallback={<Heading>Release to upload</Heading>}>
            <Heading size="lg" textAlign="center">
              Drag files here to upload, or click:
            </Heading>
            <HStack spacing="$4">
              <VStack spacing="$2" alignItems="center">
                <IconButton
                  compact
                  size="xl"
                  aria-label="Select folder"
                  colorScheme="accent"
                  icon={<RiDocumentFolderUploadFill size="1.2em" />}
                  onClick={() => folderInput.click()}
                />
                <Text fontSize="$sm" color="$neutral11" textAlign="center">Select folder</Text>
              </VStack>
              <VStack spacing="$2" alignItems="center">
                <IconButton
                  compact
                  size="xl"
                  aria-label="Select files"
                  icon={<RiDocumentFileUploadFill size="1.2em" />}
                  onClick={() => fileInput.click()}
                />
                <Text fontSize="$sm" color="$neutral11" textAlign="center">Select files</Text>
              </VStack>
            </HStack>
            <Stack
              spacing={{ "@initial": "$2", "@md": "$4" }}
              direction={{ "@initial": "column", "@md": "row" }}
            >
              <Checkbox
                checked={uploadConfig.asTask}
                onChange={() => setUploadConfig({ asTask: !uploadConfig.asTask })}
              >
                Add as task
              </Checkbox>
              <Checkbox
                checked={uploadConfig.overwrite}
                onChange={() => setUploadConfig({ overwrite: !uploadConfig.overwrite })}
              >
                Overwrite existing files
              </Checkbox>
            </Stack>
          </Show>
        </VStack>
      </Show>
    </VStack>
  )
}

export default Upload
