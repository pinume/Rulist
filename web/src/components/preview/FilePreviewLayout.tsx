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
import { ArchivePreview } from "./ArchivePreview"
import { AudioPreview } from "./AudioPreview"
import { CodePreview } from "./CodePreview"
import { CsvPreview } from "./CsvPreview"
import { HtmlPreview } from "./HtmlPreview"
import { ImagePreview } from "./ImagePreview"
import { JsonPreview } from "./JsonPreview"
import { MarkdownPreview } from "./MarkdownPreview"
import { PdfPreview } from "./PdfPreview"
import { PreviewHeader } from "./PreviewHeader"
import { UnsupportedPreview } from "./UnsupportedPreview"
import { VideoPreview } from "./VideoPreview"
import { XmlPreview } from "./XmlPreview"

export const FilePreviewLayout = (props: {
  meta: PreviewMeta
  content?: ProcessedContent
  error?: string
}) => {
  const t = useT()
  const cardBg = useColorModeValue("white", "$neutral3")

  const startDownload = () => {
    const url = new URL(props.meta.raw_url, window.location.href)
    url.searchParams.set("openlist_ts", Date.now().toString())
    const anchor = document.createElement("a")
    anchor.href = url.toString()
    anchor.download = props.meta.name
    anchor.click()
  }

  const errorMessage = () => {
    if (props.error === "too_large") {
      return "文件体积过大（已超过 4MB 预览限制），请下载后查看。"
    }
    if (props.error === "unsupported_encoding") {
      return "非 UTF-8 编码文本，暂不支持在线预览，请下载后查看。"
    }
    if (props.error === "suspicious_compression_ratio") {
      return "压缩包压缩率异常，可能存在安全风险，禁止在线预览。"
    }
    if (props.error === "archive_error") {
      return "压缩包格式错误或文件已损坏。"
    }
    if (props.error === "resource_limit") {
      return "压缩包内项目过多或解压体积过大，超出在线预览资源限制，请下载后查看。"
    }
    return props.error || "预览加载失败，请下载后查看。"
  }

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
                {errorMessage()}
              </Text>
              <Button
                colorScheme="accent"
                leftIcon={<Icon as={FiDownload} />}
                onClick={startDownload}
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
          <VideoPreview meta={props.meta} />
        </Match>
        <Match when={props.meta.preview_type === "audio"}>
          <AudioPreview meta={props.meta} />
        </Match>
        <Match when={props.meta.preview_type === "pdf"}>
          <PdfPreview meta={props.meta} />
        </Match>
        <Match when={props.meta.preview_type === "html"}>
          <HtmlPreview meta={props.meta} />
        </Match>
        <Match
          when={
            props.meta.preview_type === "markdown" &&
            props.content?.kind === "html"
          }
        >
          <MarkdownPreview content={props.content} />
        </Match>
        <Match when={props.meta.preview_type === "json"}>
          <JsonPreview content={props.content} meta={props.meta} />
        </Match>
        <Match when={props.meta.preview_type === "xml"}>
          <XmlPreview content={props.content} meta={props.meta} />
        </Match>
        <Match
          when={
            props.content?.kind === "csv_table"
          }
        >
          <CsvPreview content={props.content} meta={props.meta} />
        </Match>
        <Match
          when={
            props.content?.kind === "archive_tree" ||
            props.meta.preview_type === "archive"
          }
        >
          <ArchivePreview content={props.content} meta={props.meta} />
        </Match>
        <Match
          when={
            props.meta.preview_type === "text" ||
            props.meta.preview_type === "code" ||
            props.meta.preview_type === "csv" ||
            props.content?.kind === "text"
          }
        >
          <CodePreview content={props.content} meta={props.meta} />
        </Match>
      </Switch>
    </Box>
  )
}
