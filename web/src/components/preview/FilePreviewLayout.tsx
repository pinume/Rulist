import {
  Box,
  Button,
  Icon,
  Text,
  useColorModeValue,
  VStack,
} from "@hope-ui/solid"
import { Match, Switch } from "solid-js"
import { FiAlertTriangle, FiDownload } from "solid-icons/fi"
import { useT } from "~/hooks"
import { PreviewMeta, ProcessedContent } from "~/types"
import { startDownload } from "~/utils"
import { AudioPreview } from "./AudioPreview"
import { CodePreview } from "./CodePreview"
import { IframePreview } from "./IframePreview"
import { ImagePreview } from "./ImagePreview"
import { MarkdownPreview } from "./MarkdownPreview"
import { PreviewHeader } from "./PreviewHeader"
import { UnsupportedPreview } from "./UnsupportedPreview"
import { VideoPreview } from "./VideoPreview"

export const FilePreviewLayout = (props: {
  path: string
  meta: PreviewMeta
  content?: ProcessedContent
  error?: string
}) => {
  const t = useT()
  const cardBg = useColorModeValue("white", "$neutral3")

  return (
    <Box
      w="$full"
      rounded="$xl"
      border="1px solid"
      borderColor="$neutral4"
      bg={cardBg()}
      shadow="$sm"
      overflow="hidden"
    >
      <PreviewHeader meta={props.meta} />
      <Switch fallback={<UnsupportedPreview meta={props.meta} />}>
        <Match when={props.error}>
          <Box p="$8" display="flex" justifyContent="center">
            <VStack
              spacing="$4"
              p="$8"
              rounded="$lg"
              border="1px solid"
              borderColor="$warning6"
              bg="$warning2"
              maxW="520px"
              w="$full"
              alignItems="center"
            >
              <Icon as={FiAlertTriangle} boxSize="$12" color="$warning9" />
              <Text
                color="$warning11"
                textAlign="center"
                fontWeight="$medium"
                fontSize="$sm"
              >
                {props.error || "预览加载失败，请下载后查看。"}
              </Text>
              <Button
                colorScheme="accent"
                leftIcon={<Icon as={FiDownload} />}
                onClick={() => startDownload(props.meta.raw_url, props.meta.name)}
              >
                {t("home.toolbar.download") || "下载"}
              </Button>
            </VStack>
          </Box>
        </Match>
        <Match when={props.meta.preview_type === "image"}>
          <ImagePreview meta={props.meta} />
        </Match>
        <Match when={props.meta.preview_type === "video"}>
          <VideoPreview meta={props.meta} path={props.path} />
        </Match>
        <Match when={props.meta.preview_type === "audio"}>
          <AudioPreview meta={props.meta} path={props.path} />
        </Match>
        <Match when={props.meta.preview_type === "pdf"}>
          <IframePreview meta={props.meta} />
        </Match>
        <Match when={props.meta.preview_type === "html"}>
          <IframePreview meta={props.meta} sandbox="" />
        </Match>
        <Match when={props.meta.preview_type === "markdown"}>
          <MarkdownPreview content={props.content} />
        </Match>
        <Match
          when={
            props.meta.preview_type === "text" ||
            props.meta.preview_type === "code" ||
            props.meta.preview_type === "json" ||
            props.meta.preview_type === "xml"
          }
        >
          <CodePreview content={props.content} meta={props.meta} />
        </Match>
      </Switch>
    </Box>
  )
}
