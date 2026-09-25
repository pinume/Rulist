import { Box } from "@hope-ui/solid"
import { PreviewMeta } from "~/types"

export const ImagePreview = (props: { meta: PreviewMeta }) => {
  return (
    <Box display="flex" justifyContent="center" p="$4" overflow="auto">
      <img
        src={props.meta.raw_url}
        alt={props.meta.name}
        style={{
          "max-height": "75vh",
          "max-width": "100%",
          "object-fit": "contain",
          "border-radius": "8px",
        }}
      />
    </Box>
  )
}
