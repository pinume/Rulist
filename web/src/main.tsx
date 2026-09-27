/* @refresh reload */
import { Router } from "@solidjs/router"
import { render } from "solid-js/web"

import { Index } from "./app"

type RulistRuntimeConfig = {
  cdn?: string
  base_path?: string
  api?: string
  main_color?: string
}

declare global {
  interface Window {
    RULIST_CONFIG: RulistRuntimeConfig
    __dynamic_base__?: string
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
