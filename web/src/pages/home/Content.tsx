import { useColorModeValue, VStack } from "@hope-ui/solid"
import { createEffect, lazy, Match, on, Suspense, Switch } from "solid-js"
import { Error, FullLoading } from "~/components"
import { useFiles, useObjTitle, useRouter } from "~/hooks"
import { fileStore, recordHistory, ViewState } from "~/store"

const Folder = lazy(() => import("./folder/Folder"))
const File = lazy(() => import("./file/File"))
export const Content = () => {
  const cardBg = useColorModeValue("white", "$neutral3")
  const { pathname } = useRouter()
  const { loadPath } = useFiles()
  let lastPathname: string
  createEffect(
    on(pathname, async (pathname) => {
      if (lastPathname) recordHistory(lastPathname)
      lastPathname = pathname
      useObjTitle()
      await loadPath(pathname)
    }),
  )

  return (
    <VStack
      class="obj-box"
      w="$full"
      rounded="$xl"
      bgColor={cardBg()}
      p="$0"
      border="1px solid"
      borderColor="$neutral4"
      shadow="$sm"
      overflow="visible"
      spacing="$0"
    >
      <Suspense fallback={<FullLoading />}>
        <Switch>
          <Match when={fileStore.err}>
            <Error msg={fileStore.err} />
          </Match>
          <Match when={[ViewState.Loading, ViewState.Loading].includes(fileStore.state)}>
            <FullLoading />
          </Match>
          <Match when={fileStore.state === ViewState.Folder}>
            <Folder />
          </Match>
          <Match when={fileStore.state === ViewState.File}>
            <File />
          </Match>
        </Switch>
      </Suspense>
    </VStack>
  )
}
