import {
  Badge,
  Box,
  HStack,
  Icon,
  Input,
  InputGroup,
  InputLeftElement,
  Text,
  useColorModeValue,
  VStack,
} from "@hope-ui/solid"
import { createMemo, createSignal, For, Show } from "solid-js"
import { FiFile, FiFolder, FiSearch } from "solid-icons/fi"
import { PreviewMeta, ProcessedContent } from "~/types"
import { getFileSize } from "~/utils"

interface ArchiveEntry {
  name: string
  size: number
  is_dir: boolean
  modified?: string
}

interface ArchiveData {
  entries: ArchiveEntry[]
  total_entries: number
  total_size: number
}

export const ArchivePreview = (props: {
  content?: ProcessedContent
  meta?: PreviewMeta
}) => {
  const [filter, setFilter] = createSignal("")

  const data = createMemo<ArchiveData>(() => {
    try {
      const parsed = JSON.parse(props.content?.value ?? "{}")
      return {
        entries: Array.isArray(parsed.entries) ? parsed.entries : [],
        total_entries:
          typeof parsed.total_entries === "number" ? parsed.total_entries : 0,
        total_size:
          typeof parsed.total_size === "number" ? parsed.total_size : 0,
      }
    } catch {
      return { entries: [], total_entries: 0, total_size: 0 }
    }
  })

  const filteredEntries = createMemo(() => {
    const q = filter().trim().toLowerCase()
    if (!q) return data().entries
    return data().entries.filter((e) => e.name.toLowerCase().includes(q))
  })

  const bg = useColorModeValue("white", "$neutral3")
  const barBg = useColorModeValue("$neutral1", "$neutral2")
  const itemHoverBg = useColorModeValue("$neutral2", "$neutral4")
  const subTextColor = useColorModeValue("$neutral10", "$neutral9")
  const folderColor = useColorModeValue("$warning9", "$warning8")
  const fileColor = useColorModeValue("$neutral8", "$neutral9")

  return (
    <Box w="$full" bg={bg()}>
      {/* Summary and Filter Bar */}
      <HStack
        justifyContent="space-between"
        alignItems="center"
        px="$4"
        py="$2_5"
        borderBottom="1px solid"
        borderColor="$neutral4"
        bg={barBg()}
        spacing="$4"
        flexWrap={{ "@initial": "wrap", "@sm": "nowrap" }}
      >
        <HStack spacing="$2" flexShrink={0}>
          <Badge variant="subtle" colorScheme="info">
            共 {data().total_entries} 项
          </Badge>
          <Badge variant="outline" colorScheme="neutral">
            解压大小 {getFileSize(data().total_size)}
          </Badge>
        </HStack>

        <Box
          minW={{ "@initial": "100%", "@sm": "240px" }}
          maxW="360px"
          flex="1"
        >
          <InputGroup size="sm">
            <InputLeftElement pointerEvents="none">
              <Icon as={FiSearch} color="$neutral8" />
            </InputLeftElement>
            <Input
              placeholder="搜索压缩包内文件..."
              value={filter()}
              onInput={(e) => setFilter(e.currentTarget.value)}
              variant="outline"
              bg={bg()}
            />
          </InputGroup>
        </Box>
      </HStack>

      {/* Entry List */}
      <Box maxH="75vh" overflowY="auto" p="$2">
        <Show
          when={filteredEntries().length > 0}
          fallback={
            <Box p="$8" textAlign="center">
              <Text size="sm" color={subTextColor()}>
                未找到匹配的文件
              </Text>
            </Box>
          }
        >
          <VStack spacing="$1" w="$full" alignItems="stretch">
            <For each={filteredEntries()}>
              {(entry) => (
                <HStack
                  justifyContent="space-between"
                  alignItems="center"
                  px="$3"
                  py="$1_5"
                  rounded="$md"
                  _hover={{ bg: itemHoverBg() }}
                  spacing="$3"
                >
                  <HStack spacing="$2" minW="0" flex="1">
                    <Icon
                      as={entry.is_dir ? FiFolder : FiFile}
                      color={entry.is_dir ? folderColor() : fileColor()}
                      boxSize="$4"
                      flexShrink={0}
                    />
                    <Text
                      fontSize="$sm"
                      fontFamily='"Maple Mono NF CN", monospace'
                      overflow="hidden"
                      css={{ whiteSpace: "nowrap", textOverflow: "ellipsis" }}
                      title={entry.name}
                    >
                      {entry.name}
                    </Text>
                  </HStack>

                  <HStack spacing="$4" flexShrink={0}>
                    {entry.modified && (
                      <Text
                        fontSize="$xs"
                        color={subTextColor()}
                        display={{ "@initial": "none", "@md": "inline" }}
                      >
                        {entry.modified}
                      </Text>
                    )}
                    <Text
                      fontSize="$xs"
                      color={subTextColor()}
                      minW="70px"
                      textAlign="right"
                      fontFamily='"Maple Mono NF CN", monospace'
                    >
                      {entry.is_dir ? "-" : getFileSize(entry.size)}
                    </Text>
                  </HStack>
                </HStack>
              )}
            </For>
          </VStack>
        </Show>
      </Box>
    </Box>
  )
}
