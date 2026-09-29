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
import { useLoading, useRouter } from "~/hooks"
import { loadConfig } from "~/store"
import { PublicConfig, Resp } from "~/types"
import { bus, handleRespWithoutAuthAndNotify, r } from "~/utils"
import { MustUser } from "./MustUser"
import "./index.css"
import { globalStyles } from "./theme"

const Home = lazy(() => import("~/pages/home/Layout"))
const Login = lazy(() => import("~/pages/login"))

const App: Component = () => {
  globalStyles()
  const isRouting = useIsRouting()
  const { to, pathname } = useRouter()
  const onTo = (path: string) => to(path)
  bus.on("to", onTo)
  onCleanup(() => bus.off("to", onTo))

  createEffect(() => bus.emit("pathname", pathname()))

  const [err, setErr] = createSignal<string[]>([])
  const [loading, data] = useLoading(async () => {
    handleRespWithoutAuthAndNotify(
      (await r.get("/public/settings")) as Resp<PublicConfig>,
      loadConfig,
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
          <Routes>
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
          <Error h="100vh" msg={`Failed to fetch settings: ${err().join(", ")}`} />
        </Match>
        <Match when={loading()}>
          <FullScreenLoading />
        </Match>
      </Switch>
    </>
  )
}

export default App
