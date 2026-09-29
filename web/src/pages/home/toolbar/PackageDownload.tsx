import "~/utils/zip-stream.js"
import streamSaver from "streamsaver"
import { useRouter } from "~/hooks"
import { api, fsLink, fsList, pathBase, pathJoin } from "~/utils"
import { selectedFiles } from "~/store"
import { createSignal, For, Show } from "solid-js"
import {
  Box,
  Button,
  Heading,
  ModalBody,
  ModalFooter,
  Progress,
  ProgressIndicator,
  Text,
  VStack,
} from "@hope-ui/solid"
import { FileEntry } from "~/types"

streamSaver.mitm = "/streamer/mitm.html"
const trimSlash = (str: string) => str.replace(/^\/+|\/+$/g, "")

interface FileItem {
  path: string
}

const PackageDownload = (props: { onClose: () => void }) => {
  const [cur, setCur] = createSignal("Initializing")
  const [status, setStatus] = createSignal(0)
  const [progress, setProgress] = createSignal({ current: 0, total: 0 })
  const { pathname } = useRouter()
  const selected = selectedFiles()

  const fetchFolderStructure = async (
    pre: string,
    obj: FileEntry,
  ): Promise<FileItem[] | string> => {
    if (!obj.is_dir) return [{ path: pathJoin(pre, obj.name) }]
    const dirPath = pathJoin(pathname(), pre, obj.name)
    const resp = await fsList(dirPath)
    if (resp.code !== 200) return resp.message
    const files: FileItem[] = []
    const subTasks: Promise<FileItem[] | string>[] = []
    for (const item of resp.data.content ?? []) {
      if (item.is_dir) {
        subTasks.push(fetchFolderStructure(pathJoin(pre, obj.name), item))
      } else {
        files.push({ path: pathJoin(pre, obj.name, item.name) })
      }
    }
    if (subTasks.length > 0) {
      const results = await Promise.all(subTasks)
      for (const res of results) {
        if (typeof res === "string") return res
        files.push(...res)
      }
    }
    return files
  }

  const [fetchings, setFetchings] = createSignal<string[]>([])

  const run = async () => {
    let saveName = pathBase(pathname())
    if (selected.length === 1) saveName = selected[0].name
    if (!saveName) saveName = "Home"

    setCur("Fetching folder structure")
    setStatus(2)
    const downFiles: FileItem[] = []
    const rootResults = await Promise.all(
      selected.map((obj) => fetchFolderStructure("", obj)),
    )
    for (const res of rootResults) {
      if (typeof res === "string") {
        setCur(`Failed to fetch folder structure: ${res}`)
        setStatus(1)
        return res
      }
      downFiles.push(...res)
    }

    if (downFiles.length === 0) {
      setCur("No files to download")
      setStatus(1)
      return
    }

    const fileStream = streamSaver.createWriteStream(`${saveName}.zip`)
    setCur("Downloading files. Do not close or refresh this page.")
    setStatus(3)
    setProgress({ current: 0, total: downFiles.length })

    const concurrency = 4
    const fetchMap = new Map<number, Promise<Response>>()

    const getFileResponse = (index: number): Promise<Response> => {
      let request = fetchMap.get(index)
      if (!request) {
        const file = downFiles[index]
        request = (async () => {
          const filePath = pathJoin(pathname(), file.path)
          const linkResp = await fsLink(filePath)
          if (linkResp.code !== 200) {
            throw new Error(
              `Failed to get link for ${file.path}: ${linkResp.message}`,
            )
          }
          const rawUrl = linkResp.data.url
          const url =
            rawUrl.startsWith("http://") || rawUrl.startsWith("https://")
              ? rawUrl
              : `${api}${rawUrl}`
          const res = await fetch(url)
          if (!res.ok) {
            throw new Error(
              `Failed to fetch ${file.path}: ${res.status} ${res.statusText}`,
            )
          }
          return res
        })()
        fetchMap.set(index, request)
      }
      return request
    }

    for (let i = 0; i < Math.min(concurrency, downFiles.length); i++) {
      getFileResponse(i)
    }

    let currentIndex = 0
    const readableZipStream = new (window as any).ZIP({
      async pull(ctrl: any) {
        if (currentIndex >= downFiles.length) {
          ctrl.close()
          return
        }
        const fileIdx = currentIndex++
        const file = downFiles[fileIdx]

        for (
          let i = fileIdx + 1;
          i < Math.min(fileIdx + 1 + concurrency, downFiles.length);
          i++
        ) {
          getFileResponse(i)
        }

        let name = trimSlash(file.path)
        if (selected.length === 1) name = name.replace(`${saveName}/`, "")

        setProgress({ current: fileIdx + 1, total: downFiles.length })
        setCur(
          `Downloading files. Do not close or refresh this page. (${fileIdx + 1}/${downFiles.length})`,
        )
        setFetchings((prev) => [...prev.slice(-3), name])

        const res = await getFileResponse(fileIdx)
        fetchMap.delete(fileIdx)
        ctrl.enqueue({ name, stream: res.body })
      },
    })

    if (window.WritableStream && readableZipStream.pipeTo) {
      return readableZipStream
        .pipeTo(fileStream)
        .then(() => {
          setCur("Download complete")
          setStatus(4)
        })
        .catch((err: any) => {
          setCur(`Archive download failed: ${err}`)
          setStatus(1)
        })
    }
  }
  run()

  return (
    <>
      <ModalBody>
        <VStack w="$full" alignItems="stretch" spacing="$3">
          <Heading size="base">{cur()}</Heading>
          <Show when={status() === 3 && progress().total > 0}>
            <Box w="$full">
              <Progress
                value={Math.round(
                  (progress().current / (progress().total || 1)) * 100,
                )}
                size="sm"
                rounded="$full"
              >
                <ProgressIndicator rounded="$full" bg="$accent9" />
              </Progress>
            </Box>
          </Show>
          <VStack w="$full" alignItems="start" spacing="$1">
            <For each={fetchings()}>
              {(name) => (
                <Text size="xs" color="$neutral10" css={{ wordBreak: "break-all" }}>
                  {name}
                </Text>
              )}
            </For>
          </VStack>
        </VStack>
      </ModalBody>
      <Show when={[1, 4].includes(status())}>
        <ModalFooter>
          <Button colorScheme="info" onClick={props.onClose}>Close</Button>
        </ModalFooter>
      </Show>
    </>
  )
}

export default PackageDownload
