import { Box, Button, HStack, Icon, Text, VStack } from "@hope-ui/solid"
import { FiDownload, FiExternalLink } from "solid-icons/fi"
import { PreviewMeta, ObjType } from "~/types"
import { getFileSize, startDownload } from "~/utils"
import { getIconByObj, getIconColorByObj } from "~/utils/icon"
import { useT } from "~/hooks"

export const UnsupportedPreview = (props: { meta: PreviewMeta }) => {
  const t = useT()

  return (
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
          as={getIconByObj({ type: ObjType.UNKNOWN, name: props.meta.name })}
          boxSize="$16"
          color={getIconColorByObj({ type: ObjType.UNKNOWN, name: props.meta.name })}
        />
        <Text fontWeight="$semibold" size="lg" textAlign="center" noOfLines={2}>
          {props.meta.name}
        </Text>
        <Text size="sm" color="$neutral10">
          {getFileSize(props.meta.size)}
        </Text>
        <Text size="sm" color="$neutral11" textAlign="center">
          {t("home.preview.unsupported") || "当前格式暂不支持在线预览"}
        </Text>
        <HStack spacing="$3" pt="$2">
          <Button
            leftIcon={<Icon as={FiDownload} />}
            colorScheme="accent"
            onClick={() => startDownload(props.meta.raw_url, props.meta.name)}
          >
            {t("home.toolbar.download") || "下载"}
          </Button>
          <Button
            as="a"
            href={props.meta.raw_url}
            target="_blank"
            rel="noopener noreferrer"
            variant="outline"
            leftIcon={<Icon as={FiExternalLink} />}
          >
            {t("home.preview.open_raw") || "直接打开"}
          </Button>
        </HStack>
      </VStack>
    </Box>
  )
}
