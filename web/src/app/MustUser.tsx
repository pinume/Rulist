import { createSignal, JSXElement, Match, Switch } from "solid-js"
import { Error, FullScreenLoading } from "~/components"
import { useFetch } from "~/hooks"
import { setCurrentUser } from "~/store"
import { SessionUser } from "~/types"
import { PResp } from "~/types"
import { r, handleResp } from "~/utils"

const MustUser = (props: { children: JSXElement }) => {
  const [loading, data] = useFetch((): PResp<SessionUser> => r.get("/me"), true)
  const [err, setErr] = createSignal<string>()
  const [ready, setReady] = createSignal(false)
  void (async () => {
    handleResp(
      await data(),
      (user) => {
        setCurrentUser(user)
        setReady(true)
      },
      setErr,
    )
  })()
  return (
    <Switch fallback={props.children}>
      <Match when={err() !== undefined}>
        <Error msg={`Failed to get current user: ${err()}`} />
      </Match>
      <Match when={loading() || !ready()}>
        <FullScreenLoading />
      </Match>
    </Switch>
  )
}

export { MustUser }
