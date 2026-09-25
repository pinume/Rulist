import {
  Badge,
  Box,
  Button,
  HStack,
  Icon,
  Tooltip,
  useColorModeValue,
} from "@hope-ui/solid"
import { createMemo, createSignal, For } from "solid-js"
import { FiCheck, FiCopy, FiCornerDownLeft } from "solid-icons/fi"
import { useT, useUtil } from "~/hooks"
import { PreviewMeta, ProcessedContent } from "~/types"

export const CodePreview = (props: {
  content?: ProcessedContent
  meta?: PreviewMeta
}) => {
  const t = useT()
  const { copy } = useUtil()
  const [wrap, setWrap] = createSignal(false)
  const [copied, setCopied] = createSignal(false)

  const lines = createMemo(() => {
    const val = props.content?.value ?? ""
    return val.split("\n")
  })

  const handleCopy = async () => {
    const val = props.content?.value ?? ""
    await copy(val)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const bg = useColorModeValue("white", "$neutral3")
  const barBg = useColorModeValue("$neutral1", "$neutral2")
  const lineNumberColor = useColorModeValue("$neutral8", "$neutral9")
  const codeColor = useColorModeValue("$neutral12", "$neutral12")
  const lineHoverBg = useColorModeValue("$neutral2", "$neutral4")

  const lineNoWidth = () =>
    `${Math.max(2, String(lines().length).length) * 9 + 20}px`

  return (
    <Box w="$full" bg={bg()}>
      {/* Toolbar */}
      <HStack
        justifyContent="space-between"
        alignItems="center"
        px="$4"
        py="$2"
        borderBottom="1px solid"
        borderColor="$neutral4"
        bg={barBg()}
      >
        <HStack spacing="$2">
          <Badge variant="subtle" colorScheme="info">
            {lines().length} 行
          </Badge>
          {props.content?.kind && props.content.kind !== "text" && (
            <Badge variant="outline" colorScheme="neutral">
              {props.content.kind.toUpperCase()}
            </Badge>
          )}
        </HStack>

        <HStack spacing="$2">
          <Tooltip label={wrap() ? "切换为单行滚动" : "切换为自动换行"}>
            <Button
              size="xs"
              variant={wrap() ? "solid" : "outline"}
              colorScheme="neutral"
              leftIcon={<Icon as={FiCornerDownLeft} />}
              onClick={() => setWrap((w) => !w)}
            >
              {wrap() ? "取消换行" : "自动换行"}
            </Button>
          </Tooltip>

          <Button
            size="xs"
            variant="solid"
            colorScheme={copied() ? "success" : "accent"}
            leftIcon={<Icon as={copied() ? FiCheck : FiCopy} />}
            onClick={handleCopy}
          >
            {copied() ? t("global.copied") || "已复制" : t("global.copy") || "复制"}
          </Button>
        </HStack>
      </HStack>

      {/* Code Area */}
      <Box
        p="$3"
        overflowX={wrap() ? "hidden" : "auto"}
        fontFamily='"Maple Mono NF CN", monospace'
        fontSize="$xs"
        lineHeight="1.6"
        color={codeColor()}
      >
        <For each={lines()}>
          {(line, index) => (
            <Box
              display="flex"
              alignItems="flex-start"
              _hover={{ bg: lineHoverBg() }}
              rounded="$xs"
            >
              <Box
                userSelect="none"
                textAlign="right"
                w={lineNoWidth()}
                pr="$3"
                color={lineNumberColor()}
                flexShrink={0}
              >
                {index() + 1}
              </Box>
              <pre
                style={{
                  flex: "1",
                  "min-width": "0",
                  margin: "0",
                  "font-family": "inherit",
                  "font-size": "inherit",
                  "line-height": "inherit",
                  "white-space": wrap() ? "pre-wrap" : "pre",
                  "word-break": wrap() ? "break-word" : "normal",
                }}
              >
                {line.length > 0 ? line : "\n"}
              </pre>
            </Box>
          )}
        </For>
      </Box>
    </Box>
  )
}
