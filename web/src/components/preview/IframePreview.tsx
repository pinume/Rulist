import { Box } from "@hope-ui/solid"
import { PreviewMeta } from "~/types"

export const IframePreview = (props: { meta: PreviewMeta; sandbox?: string }) => {
  return (
    <Box p="$2">
      <iframe
        src={props.meta.raw_url}
        title={props.meta.name}
        {...(props.sandbox !== undefined ? { sandbox: props.sandbox } : {})}
        style={{
          width: "100%",
          height: "80vh",
          border: "none",
          background: "white",
          "border-radius": "8px",
        }}
      />
    </Box>
  )
}
