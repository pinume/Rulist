import { Box, Button, HStack, Icon, Text, VStack } from "@hope-ui/solid"
import { FiDownload, FiExternalLink } from "solid-icons/fi"
import { FileType, PreviewMeta } from "~/types"
import { getFileSize, startDownload } from "~/utils"
import { getIconByFile, getIconColorByFile } from "~/utils/icon"

export const UnsupportedPreview = (props: { meta: PreviewMeta }) => (
  <Box display="flex" justifyContent="center" alignItems="center" p="$8">
    <VStack
      spacing="$4"
      p="$8"
      rounded="$lg"
      border="1px solid"
      borderColor="$neutral4"
      maxW="480px"
      w="$full"
      alignItems="center"
    >
      <Icon
        as={getIconByFile({ type: FileType.UNKNOWN, name: props.meta.name })}
        boxSize="$16"
        color={getIconColorByFile({ type: FileType.UNKNOWN, name: props.meta.name })}
      />
      <Text fontWeight="$semibold" size="lg" textAlign="center" noOfLines={2}>
        {props.meta.name}
      </Text>
      <Text size="sm" color="$neutral10">{getFileSize(props.meta.size)}</Text>
      <Text size="sm" color="$neutral11" textAlign="center">
        This file format cannot be previewed online.
      </Text>
      <HStack spacing="$3" pt="$2">
        <Button
          leftIcon={<Icon as={FiDownload} />}
          colorScheme="accent"
          onClick={() => startDownload(props.meta.raw_url, props.meta.name)}
        >
          Download
        </Button>
        <Button
          as="a"
          href={props.meta.raw_url}
          target="_blank"
          rel="noopener noreferrer"
          variant="outline"
          leftIcon={<Icon as={FiExternalLink} />}
        >
          Open raw file
        </Button>
      </HStack>
    </VStack>
  </Box>
)
