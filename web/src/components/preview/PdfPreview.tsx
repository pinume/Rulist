import { Box } from "@hope-ui/solid"
import { PreviewMeta } from "~/types"

export const PdfPreview = (props: { meta: PreviewMeta }) => {
  return (
    <Box p="$2">
      <iframe
        src={props.meta.raw_url}
        title={props.meta.name}
        style={{
          width: "100%",
          height: "80vh",
          border: "none",
          "border-radius": "8px",
        }}
      />
    </Box>
  )
}
