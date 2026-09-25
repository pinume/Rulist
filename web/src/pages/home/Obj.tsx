import { Text, useColorModeValue, VStack, Button } from "@hope-ui/solid"
import {
  createEffect,
  createMemo,
  createSignal,
  lazy,
  Match,
  on,
  Suspense,
  Switch,
} from "solid-js"
import { Error, FullLoading, LinkWithBase } from "~/components"
import { useObjTitle, usePath, useRouter, useT } from "~/hooks"
import {
  objStore,
  password,
  recordHistory,
  setPassword,
  /*layout,*/ State,
  me,
} from "~/store"
import { UserMethods } from "~/types"

const Folder = lazy(() => import("./folder/Folder"))
const File = lazy(() => import("./file/File"))
const Password = lazy(() => import("./Password"))

const [objBoxRef, setObjBoxRef] = createSignal<HTMLDivElement>()
export { objBoxRef }

export const Obj = () => {
  const t = useT()
  const cardBg = useColorModeValue("white", "$neutral3")
  const { pathname, searchParams, to } = useRouter()
  const { handlePathChange, refresh } = usePath()
  let lastPathname: string
  createEffect(
    on(pathname, async (pathname) => {
      if (searchParams["pwd"]) {
        setPassword(searchParams["pwd"])
      }
      if (lastPathname) {
        recordHistory(lastPathname)
      }
      lastPathname = pathname
      useObjTitle()
      await handlePathChange(pathname)
    }),
  )

  const isStorageError = createMemo(() => {
    const err = objStore.err
    return (
      err.includes("storage not found") || err.includes("please add a storage")
    )
  })

  const shouldShowStorageButton = createMemo(() => {
    return isStorageError() && UserMethods.is_admin(me())
  })

  const storageErrorActions = () => (
    <Button colorScheme="accent" onClick={() => to("/@settings/users")}>
      {t("global.go_to_users")}
    </Button>
  )
  return (
    <VStack
      ref={(el: HTMLDivElement) => setObjBoxRef(el)}
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
            <Error
              msg={objStore.err}
              actions={
                shouldShowStorageButton() ? storageErrorActions() : undefined
              }
            />
          </Match>
          <Match
            when={[State.FetchingObj, State.FetchingObjs].includes(
              objStore.state,
            )}
          >
            <FullLoading />
          </Match>
          <Match when={objStore.state === State.NeedPassword}>
            <Password
              title={t("home.input_password")}
              password={password}
              setPassword={setPassword}
              enterCallback={() => refresh(true)}
            >
              <Text>{t("global.have_account")}</Text>
              <Text
                color="$info9"
                as={LinkWithBase}
                href={`/@login?redirect=${encodeURIComponent(
                  location.pathname,
                )}`}
              >
                {t("global.go_login")}
              </Text>
            </Password>
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
