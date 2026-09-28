/* @refresh reload */
import { Router } from "@solidjs/router"
import { render } from "solid-js/web"

import { Index } from "./app"

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
