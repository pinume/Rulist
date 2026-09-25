import { Box } from "@hope-ui/solid"
import { PreviewMeta } from "~/types"

export const VideoPreview = (props: { meta: PreviewMeta }) => {
  return (
    <Box display="flex" justifyContent="center" p="$4">
      <video
        src={props.meta.raw_url}
        controls
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
