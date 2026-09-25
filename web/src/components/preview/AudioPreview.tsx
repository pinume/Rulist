import { Box, Icon, Text, VStack } from "@hope-ui/solid"
import { BsFileEarmarkMusicFill } from "solid-icons/bs"
import { PreviewMeta } from "~/types"
import { getFileSize } from "~/utils"

export const AudioPreview = (props: { meta: PreviewMeta }) => {
  return (
    <Box display="flex" justifyContent="center" alignItems="center" p="$8">
      <VStack
        spacing="$4"
        p="$6"
        rounded="$lg"
        border="1px solid"
        borderColor="$neutral4"
        maxW="500px"
        w="$full"
        alignItems="center"
      >
        <Icon as={BsFileEarmarkMusicFill} boxSize="$16" color="#10b981" />
        <Text fontWeight="$medium" size="lg" textAlign="center" noOfLines={2}>
          {props.meta.name}
        </Text>
        <Text size="sm" color="$neutral10">
          {getFileSize(props.meta.size)}
        </Text>
        <audio
          src={props.meta.raw_url}
          controls
          preload="metadata"
          style={{ width: "100%", "max-width": "500px" }}
        />
      </VStack>
    </Box>
  )
}
