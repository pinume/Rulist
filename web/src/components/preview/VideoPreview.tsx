import { Box } from "@hope-ui/solid"
import { PreviewMeta } from "~/types"
import { useRenewMediaUrl } from "./useRenewMediaUrl"

export const VideoPreview = (props: { meta: PreviewMeta; path: string }) => {
  const { rawUrl, onError } = useRenewMediaUrl(() => props.path, () => props.meta.raw_url)

  return (
    <Box display="flex" justifyContent="center" p="$4">
      <video
        src={rawUrl()}
        controls
        onError={onError}
        preload="metadata"
        style={{
          "max-height": "75vh",
          width: "100%",
          "border-radius": "8px",
          background: "#000",
        }}
      />
    </Box>
  )
}
