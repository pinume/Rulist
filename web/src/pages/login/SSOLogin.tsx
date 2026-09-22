import { IconButton } from "@hope-ui/solid"
import { FiLogIn } from "solid-icons/fi"
import { base_path, changeToken, r } from "~/utils"
import { getSettingBool } from "~/store"
import { useRouter, useT } from "~/hooks"
import { onCleanup } from "solid-js"

const SSOLogin = () => {
  let authPopup: Window | null = null
  const ssoSignEnabled = getSettingBool("sso_login_enabled")
  const t = useT()
  const { searchParams, to } = useRouter()
  const token = searchParams["token"]
  if (token != undefined && token != "") {
    changeToken(token)
    to(decodeURIComponent(searchParams.redirect || base_path || "/"), true)
  }
  function messageEvent(event: MessageEvent) {
    if (event.origin !== window.location.origin || event.source !== authPopup)
      return
    const data = event.data
    if (data.token) {
      changeToken(data.token)
      to(decodeURIComponent(searchParams.redirect || base_path || "/"), true)
    }
  }
  window.addEventListener("message", messageEvent)
  onCleanup(() => {
    window.removeEventListener("message", messageEvent)
  })
  if (ssoSignEnabled) {
    const login = () => {
      const url = r.getUri() + "/auth/sso?method=sso_get_token"
      authPopup = window.open(url, "authPopup", "width=500,height=600")
    }
    return (
      <IconButton
        type="button"
        aria-label={t("login.sso_login")}
        icon={<FiLogIn />}
        boxSize="$8"
        compact
        onClick={login}
      />
    )
  }
}

export { SSOLogin }
