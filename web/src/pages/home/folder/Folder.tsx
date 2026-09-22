import { lazy, Switch, Match } from "solid-js"
import { layout } from "~/store"
import { ContextMenu } from "./context-menu"

const ListLayout = lazy(() => import("./List"))
const GridLayout = lazy(() => import("./Grid"))

const Folder = () => {
  return (
    <>
      <Switch>
        <Match when={layout() === "list"}>
          <ListLayout />
        </Match>
        <Match when={layout() === "grid"}>
          <GridLayout />
        </Match>
      </Switch>
      <ContextMenu />
    </>
  )
}

export default Folder
