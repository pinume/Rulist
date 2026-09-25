import {
  Divider,
  HStack,
  Icon,
  Menu,
  MenuContent,
  MenuGroup,
  MenuItem,
  MenuLabel,
  MenuTrigger,
  Text,
  useColorModeValue,
  VStack,
} from "@hope-ui/solid"
import { For, Show } from "solid-js"
import { usePath, useT, useRouter } from "~/hooks"
import {
  countMsg,
  directoryFilter,
  getMainColor,
  local,
  saveSortState,
  setLocal,
  ObjStore,
  OrderBy,
  objStore,
  selectedMsg,
  visibleObjIndexes,
} from "~/store"
import { Col, cols, ListItem } from "./ListItem"
import { useSelectWithMouse } from "./helper"
import { bus } from "~/utils"
import { BsFilter } from "solid-icons/bs"
import { FiCheck } from "solid-icons/fi"

export const ListTitle = (props: {
  sortCallback: (orderBy: OrderBy, reverse: boolean) => void
  initialOrder: OrderBy
  initialReverse: boolean
}) => {
  const t = useT()
  const { pathname } = useRouter()
  const hasNav = () => pathname().split("/").filter(Boolean).length > 0

  const updateSort = (nextOrder: OrderBy, nextReverse: boolean) => {
    saveSortState(pathname(), { orderBy: nextOrder, reverse: nextReverse })
    props.sortCallback(nextOrder, nextReverse)
  }

  const itemProps = (col: Col) => {
    return {
      fontWeight: "semibold",
      fontSize: "$xs",
      color: "$neutral9",
      textTransform: "uppercase" as any,
      letterSpacing: "0.05em",
      textAlign: col.textAlign as any,
      cursor: "pointer",
      onClick: () => {
        if (col.name === props.initialOrder) {
          updateSort(col.name as OrderBy, !props.initialReverse)
        } else {
          updateSort(col.name as OrderBy, false)
        }
      },
    }
  }
  return (
    <HStack
      class="title"
      w="$full"
      px="$3"
      py="$2"
      borderBottom="1px solid"
      borderColor="$neutral4"
      bgColor={useColorModeValue("$neutral2", "$neutral4")()}
      position="sticky"
      top={hasNav() ? "96px" : "60px"}
      zIndex={80}
      borderTopRadius="$xl"
    >
      <HStack w={cols[0].w} spacing="$1">
        {selectedMsg() ? (
          <Text {...itemProps(cols[0])}>{selectedMsg()}</Text>
        ) : (
          <Text {...itemProps(cols[0])}>{t(`home.obj.${cols[0].name}`)}</Text>
        )}
      </HStack>
      <Text
        w={cols[1].w}
        display={{ "@initial": "none", "@md": "inline" }}
        {...itemProps(cols[1])}
      >
        {t(`home.obj.${cols[1].name}`)}
      </Text>
      <Text
        w={cols[2].w}
        {...itemProps(cols[2])}
        display={{ "@initial": "none", "@md": "inline" }}
      >
        {t(`home.obj.${cols[2].name}`)}
      </Text>
      <HStack
        w={cols[3].w}
        minW={cols[3].minW}
        justifyContent="flex-end"
        flexShrink={0}
      >
        <Menu placement="bottom-end">
          <MenuTrigger
            px="$1_5"
            py="$1"
            h="auto"
            minW="unset"
            rounded="$md"
            cursor="pointer"
            bg="transparent"
            _hover={{
              bgColor: useColorModeValue("$neutral3", "$neutral5")(),
            }}
          >
            <HStack spacing="$1" alignItems="center">
              <Icon as={BsFilter} boxSize="$4" color="$neutral9" />
              <Text
                fontSize="$xs"
                fontWeight="semibold"
                color="$neutral9"
                display={{ "@initial": "none", "@md": "inline" }}
              >
                {t("home.sort") || "排序"}
              </Text>
            </HStack>
          </MenuTrigger>
          <MenuContent minW="160px" shadow="$md" zIndex={100}>
            <MenuGroup>
              <MenuLabel fontSize="$xs" color="$neutral9" px="$3" py="$1">
                {t("home.sort_by") || "排序依据"}
              </MenuLabel>
              <MenuItem
                cursor="pointer"
                onSelect={() => updateSort("name", objStore.reverse)}
              >
                <HStack
                  w="$full"
                  justifyContent="space-between"
                  alignItems="center"
                >
                  <Text
                    fontSize="$sm"
                    color={
                      objStore.orderBy === "name" ? getMainColor() : undefined
                    }
                    fontWeight={
                      objStore.orderBy === "name" ? "semibold" : "normal"
                    }
                  >
                    {t("home.sort_name") || "文件名称"}
                  </Text>
                  <Show when={objStore.orderBy === "name"}>
                    <Icon as={FiCheck} color={getMainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
              <MenuItem
                cursor="pointer"
                onSelect={() => updateSort("modified", objStore.reverse)}
              >
                <HStack
                  w="$full"
                  justifyContent="space-between"
                  alignItems="center"
                >
                  <Text
                    fontSize="$sm"
                    color={
                      objStore.orderBy === "modified"
                        ? getMainColor()
                        : undefined
                    }
                    fontWeight={
                      objStore.orderBy === "modified" ? "semibold" : "normal"
                    }
                  >
                    {t("home.sort_modified") || "修改时间"}
                  </Text>
                  <Show when={objStore.orderBy === "modified"}>
                    <Icon as={FiCheck} color={getMainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
            </MenuGroup>

            <Divider my="$1" />

            <MenuGroup>
              <MenuLabel fontSize="$xs" color="$neutral9" px="$3" py="$1">
                {t("home.sort_order") || "排序方向"}
              </MenuLabel>
              <MenuItem
                cursor="pointer"
                onSelect={() => updateSort(objStore.orderBy, false)}
              >
                <HStack
                  w="$full"
                  justifyContent="space-between"
                  alignItems="center"
                >
                  <Text
                    fontSize="$sm"
                    color={!objStore.reverse ? getMainColor() : undefined}
                    fontWeight={!objStore.reverse ? "semibold" : "normal"}
                  >
                    {t("home.sort_asc") || "A 至 Z"}
                  </Text>
                  <Show when={!objStore.reverse}>
                    <Icon as={FiCheck} color={getMainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
              <MenuItem
                cursor="pointer"
                onSelect={() => updateSort(objStore.orderBy, true)}
              >
                <HStack
                  w="$full"
                  justifyContent="space-between"
                  alignItems="center"
                >
                  <Text
                    fontSize="$sm"
                    color={
                      objStore.reverse ? getMainColor() : undefined
                    }
                    fontWeight={
                      objStore.reverse ? "semibold" : "normal"
                    }
                  >
                    {t("home.sort_desc") || "Z 至 A"}
                  </Text>
                  <Show when={objStore.reverse}>
                    <Icon as={FiCheck} color={getMainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
            </MenuGroup>

            <Divider my="$1" />

            <MenuGroup>
              <MenuLabel fontSize="$xs" color="$neutral9" px="$3" py="$1">
                {t("home.sort_folder") || "文件夹"}
              </MenuLabel>
              <MenuItem
                cursor="pointer"
                onSelect={() => {
                  setLocal("folder_sort_position", "top")
                  props.sortCallback(objStore.orderBy, objStore.reverse)
                }}
              >
                <HStack
                  w="$full"
                  justifyContent="space-between"
                  alignItems="center"
                >
                  <Text
                    fontSize="$sm"
                    color={
                      (local["folder_sort_position"] || "top") === "top"
                        ? getMainColor()
                        : undefined
                    }
                    fontWeight={
                      (local["folder_sort_position"] || "top") === "top"
                        ? "semibold"
                        : "normal"
                    }
                  >
                    {t("home.sort_folder_top") || "顶部"}
                  </Text>
                  <Show
                    when={(local["folder_sort_position"] || "top") === "top"}
                  >
                    <Icon as={FiCheck} color={getMainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
              <MenuItem
                cursor="pointer"
                onSelect={() => {
                  setLocal("folder_sort_position", "mixed")
                  props.sortCallback(objStore.orderBy, objStore.reverse)
                }}
              >
                <HStack
                  w="$full"
                  justifyContent="space-between"
                  alignItems="center"
                >
                  <Text
                    fontSize="$sm"
                    color={
                      local["folder_sort_position"] === "mixed"
                        ? getMainColor()
                        : undefined
                    }
                    fontWeight={
                      local["folder_sort_position"] === "mixed"
                        ? "semibold"
                        : "normal"
                    }
                  >
                    {t("home.sort_folder_mixed") || "与文件混合"}
                  </Text>
                  <Show when={local["folder_sort_position"] === "mixed"}>
                    <Icon as={FiCheck} color={getMainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
            </MenuGroup>
          </MenuContent>
        </Menu>
      </HStack>
    </HStack>
  )
}

const ListLayout = () => {
  const t = useT()
  const { pathname } = useRouter()
  const { handleFolder } = usePath()

  const { registerSelectContainer } = useSelectWithMouse()
  registerSelectContainer()

  const onDragOver = (e: DragEvent) => {
    const items = Array.from(e.dataTransfer?.items ?? [])
    for (let i = 0; i < items.length; i++) {
      const item = items[i]
      if (item.kind === "file") {
        bus.emit("tool", "upload")
        e.preventDefault()
        break
      }
    }
  }

  return (
    <VStack
      onDragOver={onDragOver}
      class="list viselect-container"
      w="$full"
      spacing="$0"
    >
      <ListTitle
        sortCallback={(orderBy, reverse) => {
          ObjStore.setSort(orderBy, reverse)
          void handleFolder(pathname(), false, 1, orderBy, reverse)
        }}
        initialOrder={objStore.orderBy}
        initialReverse={objStore.reverse}
      />
      <For each={visibleObjIndexes()}>
        {(index) => {
          return <ListItem obj={objStore.objs[index]} index={index} />
        }}
      </For>
      <Show when={directoryFilter().trim() && visibleObjIndexes().length === 0}>
        <Text size="sm" color="$neutral11" p="$4">
          {t("home.search.empty")}
        </Text>
      </Show>
      <HStack
        w="$full"
        px="$3"
        py="$2"
        borderTop="1px solid"
        borderColor="$neutral4"
        bgColor={useColorModeValue("$neutral1", "$neutral3")()}
        justifyContent="space-between"
        alignItems="center"
        position="sticky"
        bottom={0}
        zIndex={80}
        borderBottomRadius="$xl"
        shadow="$sm"
      >
        <Text size="xs" color="$neutral10">
          {directoryFilter().trim()
            ? t("home.search.results", { count: visibleObjIndexes().length })
            : countMsg()}
        </Text>
      </HStack>
    </VStack>
  )
}

export default ListLayout
