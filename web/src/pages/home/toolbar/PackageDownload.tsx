import "~/utils/zip-stream.js"
import streamSaver from "streamsaver"
import { useRouter, useT } from "~/hooks"
import { api, fsLink, fsList, joinBase, pathBase, pathJoin } from "~/utils"
import { selectedObjs as _selectedObjs } from "~/store"
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
import { Obj } from "~/types"

streamSaver.mitm = joinBase("streamer", "mitm.html")
const trimSlash = (str: string) => {
  return str.replace(/^\/+|\/+$/g, "")
}

interface FileItem {
  path: string
  url?: string
}

const PackageDownload = (props: { onClose: () => void }) => {
  const t = useT()
  const [cur, setCur] = createSignal(t("home.package_download.initializing"))
  // 0: init, 1: error, 2: fetching structure, 3: fetching files, 4: success
  const [status, setStatus] = createSignal(0)
  const [progress, setProgress] = createSignal({ current: 0, total: 0 })
  const { pathname } = useRouter()
  const selectedObjs = _selectedObjs()

  const fetchFolderStructure = async (
    pre: string,
    obj: Obj,
  ): Promise<FileItem[] | string> => {
    if (!obj.is_dir) {
      return [
        {
          path: pathJoin(pre, obj.name),
          url: obj.raw_url,
        },
      ]
    }
    const dirPath = pathJoin(pathname(), pre, obj.name)
    const resp = await fsList(dirPath)
    if (resp.code !== 200) {
      return resp.message
    }
    const files: FileItem[] = []
    const subTasks: Promise<FileItem[] | string>[] = []
    for (const item of resp.data.content ?? []) {
      if (item.is_dir) {
        subTasks.push(fetchFolderStructure(pathJoin(pre, obj.name), item))
      } else {
        files.push({
          path: pathJoin(pre, obj.name, item.name),
          url: item.raw_url,
        })
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
    if (selectedObjs.length === 1) {
      saveName = selectedObjs[0].name
    }
    if (!saveName) {
      saveName = t("global.home")
    }

    setCur(t("home.package_download.fetching_struct"))
    setStatus(2)
    const downFiles: FileItem[] = []
    const rootTasks = selectedObjs.map((obj) => fetchFolderStructure("", obj))
    const rootResults = await Promise.all(rootTasks)
    for (const res of rootResults) {
      if (typeof res === "string") {
        setCur(`${t("home.package_download.fetching_struct_failed")}: ${res}`)
        setStatus(1)
        return res
      }
      downFiles.push(...res)
    }

    if (downFiles.length === 0) {
      setCur("没有需要下载的文件")
      setStatus(1)
      return
    }

    const fileStream = streamSaver.createWriteStream(`${saveName}.zip`)
    setCur(t("home.package_download.downloading"))
    setStatus(3)
    setProgress({ current: 0, total: downFiles.length })

    const CONCURRENCY = 4
    const fetchMap = new Map<number, Promise<Response>>()

    const getFileResponse = (index: number): Promise<Response> => {
      let p = fetchMap.get(index)
      if (!p) {
        const file = downFiles[index]
        p = (async () => {
          let rawUrl = file.url
          if (!rawUrl) {
            const filePath = pathJoin(pathname(), file.path)
            const linkResp = await fsLink(filePath)
            if (linkResp.code !== 200) {
              throw new Error(
                `Failed to get link for ${file.path}: ${linkResp.message}`,
              )
            }
            rawUrl = linkResp.data.url
          }
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
        fetchMap.set(index, p)
      }
      return p
    }

    // Pre-start fetching the first few files
    for (let i = 0; i < Math.min(CONCURRENCY, downFiles.length); i++) {
      getFileResponse(i)
    }

    let currentIndex = 0
    let readableZipStream = new (window as any).ZIP({
      async pull(ctrl: any) {
        if (currentIndex >= downFiles.length) {
          ctrl.close()
          return
        }
        const fileIdx = currentIndex++
        const file = downFiles[fileIdx]

        // Keep the prefetch pipeline filled
        for (
          let i = fileIdx + 1;
          i < Math.min(fileIdx + 1 + CONCURRENCY, downFiles.length);
          i++
        ) {
          getFileResponse(i)
        }

        let name = trimSlash(file.path)
        if (selectedObjs.length === 1) {
          name = name.replace(`${saveName}/`, "")
        }

        setProgress({ current: fileIdx + 1, total: downFiles.length })
        setCur(
          `${t("home.package_download.downloading")} (${fileIdx + 1}/${downFiles.length})`,
        )
        setFetchings((prev) => [...prev.slice(-3), name])

        const res = await getFileResponse(fileIdx)
        fetchMap.delete(fileIdx)

        ctrl.enqueue({
          name,
          stream: res.body,
        })
      },
    })

    if (window.WritableStream && readableZipStream.pipeTo) {
      return readableZipStream
        .pipeTo(fileStream)
        .then(() => {
          setCur(`${t("home.package_download.success")}`)
          setStatus(4)
        })
        .catch((err: any) => {
          setCur(`${t("home.package_download.failed")}: ${err}`)
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
                <Text
                  size="xs"
                  color="$neutral10"
                  css={{
                    wordBreak: "break-all",
                  }}
                >
                  {name}
                </Text>
              )}
            </For>
          </VStack>
        </VStack>
      </ModalBody>
      <Show when={[1, 4].includes(status())}>
        <ModalFooter>
          <Button colorScheme="info" onClick={props.onClose}>
            {t("global.close")}
          </Button>
        </ModalFooter>
      </Show>
    </>
  )
}

export default PackageDownload
