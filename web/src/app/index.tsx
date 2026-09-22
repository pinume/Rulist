import {
  HopeProvider,
  NotificationsProvider,
  useColorMode,
} from "@hope-ui/solid"
import { ErrorBoundary, onCleanup, onMount, Suspense } from "solid-js"
import { Error, FullScreenLoading } from "~/components"
import App from "./App"
import { globalStyles, theme } from "./theme"

try {
  localStorage.removeItem("hope-ui-color-mode")
} catch {}

const SystemColorMode = () => {
  const { setColorMode } = useColorMode()

  onMount(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)")
    const syncColorMode = () => setColorMode(media.matches ? "dark" : "light")
    syncColorMode()
    media.addEventListener("change", syncColorMode)
    onCleanup(() => media.removeEventListener("change", syncColorMode))
  })

  return null
}

const Index = () => {
  globalStyles()
  return (
    <HopeProvider config={theme}>
      <SystemColorMode />
      <ErrorBoundary
        fallback={(err) => {
          console.error("error", err)
          return <Error msg={`System error: ${err}`} h="100vh" />
        }}
      >
        <NotificationsProvider duration={3000}>
          <Suspense fallback={<FullScreenLoading />}>
            <App />
          </Suspense>
        </NotificationsProvider>
      </ErrorBoundary>
    </HopeProvider>
  )
}

export { Index }
