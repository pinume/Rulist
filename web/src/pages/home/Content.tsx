import { useColorModeValue, VStack } from "@hope-ui/solid"
import { createEffect, lazy, Match, on, Suspense, Switch } from "solid-js"
import { Error, FullLoading } from "~/components"
import { useObjTitle, usePath, useRouter } from "~/hooks"
import { objStore, recordHistory, State } from "~/store"

const Folder = lazy(() => import("./folder/Folder"))
const File = lazy(() => import("./file/File"))
export const Content = () => {
  const cardBg = useColorModeValue("white", "$neutral3")
  const { pathname } = useRouter()
  const { handlePathChange } = usePath()
  let lastPathname: string
  createEffect(
    on(pathname, async (pathname) => {
      if (lastPathname) recordHistory(lastPathname)
      lastPathname = pathname
      useObjTitle()
      await handlePathChange(pathname)
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
          <Match when={objStore.err}>
            <Error msg={objStore.err} />
          </Match>
          <Match when={[State.FetchingObj, State.FetchingObjs].includes(objStore.state)}>
            <FullLoading />
          </Match>
          <Match when={objStore.state === State.Folder}>
            <Folder />
          </Match>
          <Match when={objStore.state === State.File}>
            <File />
          </Match>
        </Switch>
      </Suspense>
    </VStack>
  )
}
