import { Button, Spinner, Text, VStack } from "@hope-ui/solid"
import { createResource, Match, Switch } from "solid-js"
import { FilePreviewLayout } from "~/components/preview"
import { useRouter } from "~/hooks"
import { fileStore } from "~/store"
import { PreviewResponse, Resp } from "~/types"
import { downloadPath, getFileSize, notify, r } from "~/utils"

const fetchPreview = async (path: string): Promise<PreviewResponse | null> => {
  if (!path) return null
  try {
    const resp: Resp<PreviewResponse> = await r.post("/fs/preview", { path })
    if (resp.code === 200 && resp.data?.meta) return resp.data
  } catch (e) {
    console.error("Failed to load file preview:", e)
  }
  return null
}

const isOfficeFile = (path: string) =>
  /\.(doc|docx|docm|wps|rtf|odt|dot|dotx|dotm|xls|xlsx|xlsb|xlsm|et|xlt|xltx|xltm|xlam|ppt|pptx|pptm|pps|ppsx|ppsm|dps|key|pot|potx|potm)$/i.test(
    path,
  )

const File = () => {
  const { pathname } = useRouter()
  const [preview] = createResource(pathname, fetchPreview)

  return (
    <Switch>
      <Match when={!isOfficeFile(pathname()) && preview.loading}>
        <VStack
          w="$full"
          minH="25vh"
          py="$8"
          justifyContent="center"
          alignItems="center"
        >
          <Spinner size="xl" thickness="3px" color="$accent9" />
        </VStack>
      </Match>
      <Match when={!isOfficeFile(pathname()) && preview()?.meta}>
        <FilePreviewLayout
          path={pathname()}
          meta={preview()!.meta}
          content={preview()!.content}
          error={preview()!.error}
        />
      </Match>
      <Match when={true}>
        <VStack
          w="$full"
          minH="25vh"
          py="$8"
          px="$4"
          justifyContent="center"
          spacing="$3"
        >
          <Text fontWeight="$medium">
            {preview()?.meta?.name || fileStore.file.name}
          </Text>
          <Text color="$neutral10">
            {getFileSize(preview()?.meta?.size || fileStore.file.size)}
          </Text>
          <Button
            onClick={() => {
              const path = preview()?.meta?.path || pathname()
              const name =
                preview()?.meta?.name || fileStore.file.name || "download"
              void downloadPath(path, name).catch((err) =>
                notify.error(err instanceof Error ? err.message : String(err)),
              )
            }}
          >
            Download
          </Button>
        </VStack>
      </Match>
    </Switch>
  )
}

export default File
