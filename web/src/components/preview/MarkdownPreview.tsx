import { Box, useColorModeValue } from "@hope-ui/solid"
import { ProcessedContent } from "~/types"

export const MarkdownPreview = (props: { content?: ProcessedContent }) => {
  const bg = useColorModeValue("white", "$neutral3")
  const textColor = useColorModeValue("$neutral12", "$neutral12")

  return (
    <Box w="$full" p="$6" bg={bg()} color={textColor()} overflowX="auto">
      <Box
        class="markdown-body"
        innerHTML={props.content?.value ?? ""}
      />
    </Box>
  )
}
