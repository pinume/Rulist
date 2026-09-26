import { createEffect, createSignal } from "solid-js"
import { PreviewResponse, Resp } from "~/types"
import { r } from "~/utils"

const isExpired = (rawUrl: string) => {
  const sign = new URL(rawUrl, window.location.href).searchParams.get("sign")
  const expires = Number(sign?.slice(sign.lastIndexOf(":") + 1))
  return Number.isFinite(expires) && expires > 0 && expires < Date.now() / 1000
}

export const useRenewMediaUrl = (path: () => string, initialUrl: () => string) => {
  const [rawUrl, setRawUrl] = createSignal(initialUrl())
  let renewedUrl = ""

  createEffect(() => setRawUrl(initialUrl()))

  const onError = async (event: Event) => {
    const currentUrl = rawUrl()
    if (renewedUrl === currentUrl || !isExpired(currentUrl)) return
    renewedUrl = currentUrl

    const media = event.currentTarget as HTMLMediaElement
    const currentTime = media.currentTime
    const wasPlaying = !media.paused
    const resp: Resp<PreviewResponse> = await r.post("/fs/preview", { path: path() })
    const nextUrl = resp.code === 200 ? resp.data?.meta?.raw_url : undefined
    if (!nextUrl || nextUrl === currentUrl) return

    const restore = () => {
      media.currentTime = currentTime
      if (wasPlaying) void media.play().catch(() => {})
    }
    media.addEventListener("loadedmetadata", restore, { once: true })
    setRawUrl(nextUrl)
    media.load()
  }

  return { rawUrl, onError }
}
