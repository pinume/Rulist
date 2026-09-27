/* @refresh reload */
import { Router } from "@solidjs/router"
import { render } from "solid-js/web"

import { Index } from "./app"

type RulistRuntimeConfig = {
  api?: string
  main_color?: string
}

declare global {
  interface Window {
    RULIST_CONFIG: RulistRuntimeConfig
  }
}

declare module "solid-js" {
  namespace JSX {
    interface CustomEvents extends HTMLElementEventMap {}
    interface CustomCaptureEvents extends HTMLElementEventMap {}
  }
}

render(
  () => (
    <Router>
      <Index />
    </Router>
  ),
  document.getElementById("root") as HTMLElement,
)
