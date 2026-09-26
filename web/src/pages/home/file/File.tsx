import { Button, Spinner, Text, VStack } from "@hope-ui/solid"
import { createResource, Match, Switch } from "solid-js"
import { FilePreviewLayout } from "~/components/preview"
import { useRouter, useT } from "~/hooks"
import { objStore } from "~/store"
import { PreviewResponse, Resp } from "~/types"
import { getFileSize, r, startDownload } from "~/utils"

const fetchPreview = async (path: string): Promise<PreviewResponse | null> => {
  if (!path) return null
  try {
    const resp: Resp<PreviewResponse> = await r.post("/fs/preview", { path })
    if (resp.code === 200 && resp.data?.meta) {
      return resp.data
    }
  } catch (e) {
    console.error("Failed to load file preview:", e)
  }
  return null
}

const File = () => {
  const t = useT()
  const { pathname } = useRouter()
  const [preview] = createResource(pathname, fetchPreview)

  return (
    <Switch>
      <Match when={preview.loading}>
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
      <Match when={preview()?.meta}>
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
            {preview()?.meta?.name || objStore.obj.name}
          </Text>
          <Text color="$neutral10">
            {getFileSize(preview()?.meta?.size || objStore.obj.size)}
          </Text>
          <Button
            onClick={() =>
              startDownload(
                preview()?.meta?.raw_url || objStore.raw_url,
                preview()?.meta?.name || objStore.obj.name || "download",
              )
            }
          >
            {t("home.toolbar.download")}
          </Button>
        </VStack>
      </Match>
    </Switch>
  )
}

export default File
