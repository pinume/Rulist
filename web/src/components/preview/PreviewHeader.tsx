import {
  Box,
  Button,
  HStack,
  Icon,
  IconButton,
  Text,
  Tooltip,
  useColorModeValue,
} from "@hope-ui/solid"
import { FiDownload, FiExternalLink } from "solid-icons/fi"
import { PreviewMeta, ObjType } from "~/types"
import { formatDate, getFileSize, startDownload } from "~/utils"
import { getIconByObj, getIconColorByObj } from "~/utils/icon"
import { useT } from "~/hooks"

export const PreviewHeader = (props: { meta: PreviewMeta }) => {
  const t = useT()
  const headerBg = useColorModeValue("$neutral1", "$neutral2")
  const metaColor = useColorModeValue("$neutral10", "$neutral9")

  const metaDetails = () => {
    const parts = [
      props.meta.mime_type,
      getFileSize(props.meta.size),
      formatDate(props.meta.modified),
    ]
    if (props.meta.permissions) {
      parts.push(props.meta.permissions)
    }
    return parts.join(" · ")
  }

  return (
    <HStack
      justifyContent="space-between"
      alignItems="center"
      p="$3"
      px="$4"
      borderBottom="1px solid"
      borderColor="$neutral4"
      bg={headerBg()}
      w="$full"
      wrap="wrap"
      gap="$2"
    >
      <HStack spacing="$3" minW="0" flex="1">
        <Icon
          as={getIconByObj({ type: ObjType.UNKNOWN, name: props.meta.name })}
          color={getIconColorByObj({ type: ObjType.UNKNOWN, name: props.meta.name })}
          boxSize="$7"
          flexShrink={0}
        />
        <Box minW="0" flex="1">
          <Text
            fontWeight="$semibold"
            fontSize="$sm"
            noOfLines={1}
            title={props.meta.name}
          >
            {props.meta.name}
          </Text>
          <Text fontSize="$xs" color={metaColor()} noOfLines={1}>
            {metaDetails()}
          </Text>
        </Box>
      </HStack>

      <HStack spacing="$2" flexShrink={0}>
        <Tooltip label={t("home.toolbar.download") || "Download"}>
          <Button
            size="sm"
            colorScheme="accent"
            leftIcon={<Icon as={FiDownload} />}
            onClick={() => startDownload(props.meta.raw_url, props.meta.name)}
          >
            {t("home.toolbar.download") || "Download"}
          </Button>
        </Tooltip>
        <Tooltip label="Open Raw">
          <IconButton
            as="a"
            href={props.meta.raw_url}
            target="_blank"
            rel="noopener noreferrer"
            size="sm"
            variant="ghost"
            aria-label="Open Raw"
            icon={<Icon as={FiExternalLink} />}
          />
        </Tooltip>
      </HStack>
    </HStack>
  )
}
