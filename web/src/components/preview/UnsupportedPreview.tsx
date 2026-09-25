import { Box, Button, HStack, Icon, Text, VStack } from "@hope-ui/solid"
import { FiDownload, FiExternalLink } from "solid-icons/fi"
import { PreviewMeta, ObjType } from "~/types"
import { getFileSize } from "~/utils"
import { getIconByObj, getIconColorByObj } from "~/utils/icon"
import { useT } from "~/hooks"

export const UnsupportedPreview = (props: { meta: PreviewMeta }) => {
  const t = useT()

  const startDownload = () => {
    const url = new URL(props.meta.raw_url, window.location.href)
    url.searchParams.set("openlist_ts", Date.now().toString())
    const anchor = document.createElement("a")
    anchor.href = url.toString()
    anchor.download = props.meta.name
    anchor.click()
  }

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
            onClick={startDownload}
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
