import {
  Badge,
  Box,
  HStack,
  Icon,
  Text,
  useColorModeValue,
} from "@hope-ui/solid"
import { createMemo, For, Show } from "solid-js"
import { FiAlertCircle } from "solid-icons/fi"
import { PreviewMeta, ProcessedContent } from "~/types"

interface CsvData {
  headers: string[]
  rows: string[][]
  truncated: boolean
}

export const CsvPreview = (props: {
  content?: ProcessedContent
  meta?: PreviewMeta
}) => {
  const data = createMemo<CsvData>(() => {
    try {
      const parsed = JSON.parse(props.content?.value ?? "{}")
      return {
        headers: Array.isArray(parsed.headers) ? parsed.headers : [],
        rows: Array.isArray(parsed.rows) ? parsed.rows : [],
        truncated: !!parsed.truncated,
      }
    } catch {
      return { headers: [], rows: [], truncated: false }
    }
  })

  const bg = useColorModeValue("white", "$neutral3")
  const barBg = useColorModeValue("$neutral1", "$neutral2")
  const headerBg = useColorModeValue("$neutral2", "$neutral4")
  const rowHoverBg = useColorModeValue("$neutral2", "$neutral5")
  const zebraOddBg = useColorModeValue("$white", "$neutral3")
  const zebraEvenBg = useColorModeValue("$neutral1", "$neutral4")

  return (
    <Box w="$full" bg={bg()}>
      {/* Top Toolbar */}
      <HStack
        justifyContent="space-between"
        alignItems="center"
        px="$4"
        py="$2"
        borderBottom="1px solid"
        borderColor="$neutral4"
        bg={barBg()}
      >
        <HStack spacing="$2" alignItems="center">
          <Badge variant="subtle" colorScheme="info">
            共显示 {data().rows.length} 行
          </Badge>
          <Badge variant="outline" colorScheme="neutral">
            {data().headers.length} 列
          </Badge>
          <Show when={data().truncated}>
            <HStack spacing="$1" color="$warning10" fontSize="$xs">
              <Icon as={FiAlertCircle} />
              <Text fontSize="$xs" color="$warning11">
                已达到预览最大行数限制，更多行请下载文件查看
              </Text>
            </HStack>
          </Show>
        </HStack>
      </HStack>

      {/* Table Area */}
      <Box w="$full" overflowX="auto" maxH="75vh" overflowY="auto">
        <table
          style={{
            width: "100%",
            "border-collapse": "collapse",
            "font-family": '"Maple Mono NF CN", monospace',
            "font-size": "13px",
            "text-align": "left",
          }}
        >
          <thead
            style={{
              position: "sticky",
              top: 0,
              "z-index": 2,
            }}
          >
            <tr
              style={{
                background: headerBg(),
                "border-bottom": "2px solid var(--hope-colors-neutral5)",
              }}
            >
              <th
                style={{
                  padding: "8px 12px",
                  width: "50px",
                  "text-align": "center",
                  "font-weight": "600",
                  "font-size": "11px",
                  color: "var(--hope-colors-neutral9)",
                  "border-right": "1px solid var(--hope-colors-neutral4)",
                }}
              >
                #
              </th>
              <For each={data().headers}>
                {(col, colIdx) => (
                  <th
                    style={{
                      padding: "8px 12px",
                      "font-weight": "600",
                      "white-space": "nowrap",
                      "border-right": "1px solid var(--hope-colors-neutral4)",
                    }}
                    title={col}
                  >
                    {col || `Col ${colIdx() + 1}`}
                  </th>
                )}
              </For>
            </tr>
          </thead>
          <tbody>
            <For each={data().rows}>
              {(row, rowIdx) => {
                const isEven = () => rowIdx() % 2 === 1
                return (
                  <tr
                    style={{
                      background: isEven() ? zebraEvenBg() : zebraOddBg(),
                      "border-bottom": "1px solid var(--hope-colors-neutral3)",
                    }}
                    onMouseEnter={(e) => {
                      e.currentTarget.style.background = rowHoverBg()
                    }}
                    onMouseLeave={(e) => {
                      e.currentTarget.style.background = isEven()
                        ? zebraEvenBg()
                        : zebraOddBg()
                    }}
                  >
                    <td
                      style={{
                        padding: "6px 12px",
                        "text-align": "center",
                        "font-size": "11px",
                        color: "var(--hope-colors-neutral8)",
                        "user-select": "none",
                        "border-right": "1px solid var(--hope-colors-neutral4)",
                      }}
                    >
                      {rowIdx() + 1}
                    </td>
                    <For each={data().headers}>
                      {(_, colIdx) => {
                        const cell = row[colIdx()] ?? ""
                        return (
                          <td
                            style={{
                              padding: "6px 12px",
                              "max-width": "300px",
                              overflow: "hidden",
                              "text-overflow": "ellipsis",
                              "white-space": "nowrap",
                              "border-right": "1px solid var(--hope-colors-neutral3)",
                            }}
                            title={cell}
                          >
                            {cell}
                          </td>
                        )
                      }}
                    </For>
                  </tr>
                )
              }}
            </For>
          </tbody>
        </table>
      </Box>
    </Box>
  )
}
