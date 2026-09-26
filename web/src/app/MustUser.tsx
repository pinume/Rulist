import { createSignal, JSXElement, Match, Switch } from "solid-js"
import { Error, FullScreenLoading } from "~/components"
import { useFetch, useT } from "~/hooks"
import { Me, setMe } from "~/store"
import { PResp } from "~/types"
import { r, handleResp } from "~/utils"

const MustUser = (props: { children: JSXElement }) => {
  const t = useT()
  const [loading, data] = useFetch((): PResp<Me> => r.get("/me"), true)
  const [err, setErr] = createSignal<string>()
  const [ready, setReady] = createSignal(false)
  void (async () => {
    handleResp(
      await data(),
      (user) => {
        setMe(user)
        setReady(true)
      },
      setErr,
    )
  })()
  return (
    <Switch fallback={props.children}>
      <Match when={err() !== undefined}>
        <Error msg={t("home.get_current_user_failed") + err()} />
      </Match>
      <Match when={loading() || !ready()}>
        <FullScreenLoading />
      </Match>
    </Switch>
  )
}

export { MustUser }
