import { Progress, ProgressIndicator } from "@hope-ui/solid"
import { Route, Routes, useIsRouting } from "@solidjs/router"
import {
  Component,
  createEffect,
  createSignal,
  lazy,
  Match,
  onCleanup,
  Switch,
} from "solid-js"
import { Portal } from "solid-js/web"
import { Error, FullScreenLoading } from "~/components"
import { useLoading, useRouter, useT } from "~/hooks"
import { setSettings } from "~/store"
import { Resp } from "~/types"
import { base_path, bus, handleRespWithoutAuthAndNotify, r } from "~/utils"
import { MustUser } from "./MustUser"
import "./index.css"
import { globalStyles } from "./theme"

const Home = lazy(() => import("~/pages/home/Layout"))
const Login = lazy(() => import("~/pages/login"))

const App: Component = () => {
  const t = useT()
  globalStyles()
  const isRouting = useIsRouting()
  const { to, pathname } = useRouter()
  const onTo = (path: string) => {
    to(path)
  }
  bus.on("to", onTo)
  onCleanup(() => {
    bus.off("to", onTo)
  })

  createEffect(() => {
    bus.emit("pathname", pathname())
  })

  const [err, setErr] = createSignal<string[]>([])
  const [loading, data] = useLoading(async () => {
    handleRespWithoutAuthAndNotify(
      (await r.get("/public/settings")) as Resp<Record<string, string>>,
      setSettings,
      (e) => setErr(err().concat(e)),
    )
  })
  data()
  return (
    <>
      <Portal>
        <Progress
          indeterminate
          size="xs"
          position="fixed"
          top="0"
          left="0"
          right="0"
          zIndex="$banner"
          d={isRouting() ? "block" : "none"}
        >
          <ProgressIndicator />
        </Progress>
      </Portal>
      <Switch
        fallback={
          <Routes base={base_path}>
            <Route path="/@login" component={Login} />
            <Route
              path="*"
              element={
                <MustUser>
                  <Home />
                </MustUser>
              }
            />
          </Routes>
        }
      >
        <Match when={err().length > 0}>
          <Error
            h="100vh"
            msg={
              t("home.fetching_settings_failed") +
              err()
                .map((e) => t("home." + e))
                .join(", ")
            }
          />
        </Match>
        <Match when={loading()}>
          <FullScreenLoading />
        </Match>
      </Switch>
    </>
  )
}

export default App
