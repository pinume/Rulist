import {
  Button,
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
import { useFiles, useRouter } from "~/hooks"
import {
  countMsg,
  directoryFilter,
  mainColor,
  saveSortState,
  FileStore,
  OrderBy,
  fileStore,
  selectAll,
  selectedMsg,
  can,
  visibleFileIndexes,
} from "~/store"
import { Col, cols, ListItem } from "./ListItem"
import { bus } from "~/utils"
import { BsFilter } from "solid-icons/bs"
import { FiCheck } from "solid-icons/fi"

const columnLabel: Record<string, string> = {
  name: "Name",
  size: "Size",
  modified: "Modified",
}

export const ListTitle = (props: {
  sortCallback: (orderBy: OrderBy, reverse: boolean) => void
  initialOrder: OrderBy
  initialReverse: boolean
}) => {
  const { pathname } = useRouter()
  const hasNav = () => pathname().split("/").filter(Boolean).length > 0

  const updateSort = (nextOrder: OrderBy, nextReverse: boolean) => {
    saveSortState(pathname(), { orderBy: nextOrder, reverse: nextReverse })
    props.sortCallback(nextOrder, nextReverse)
  }

  const itemProps = (col: Col) => ({
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
  })

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
        <Text {...itemProps(cols[0])}>{columnLabel[cols[0].name]}</Text>
      </HStack>
      <Text
        w={cols[1].w}
        display={{ "@initial": "none", "@md": "inline" }}
        {...itemProps(cols[1])}
      >
        {columnLabel[cols[1].name]}
      </Text>
      <Text
        w={cols[2].w}
        {...itemProps(cols[2])}
        display={{ "@initial": "none", "@md": "inline" }}
      >
        {columnLabel[cols[2].name]}
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
            _hover={{ bgColor: useColorModeValue("$neutral3", "$neutral5")() }}
          >
            <HStack spacing="$1" alignItems="center">
              <Icon as={BsFilter} boxSize="$4" color="$neutral9" />
              <Text
                fontSize="$xs"
                fontWeight="semibold"
                color="$neutral9"
                display={{ "@initial": "none", "@md": "inline" }}
              >
                Sort
              </Text>
            </HStack>
          </MenuTrigger>
          <MenuContent minW="160px" shadow="$md" zIndex={100}>
            <MenuGroup>
              <MenuLabel fontSize="$xs" color="$neutral9" px="$3" py="$1">
                Sort by
              </MenuLabel>
              <MenuItem cursor="pointer" onSelect={() => updateSort("name", fileStore.reverse)}>
                <HStack w="$full" justifyContent="space-between" alignItems="center">
                  <Text
                    fontSize="$sm"
                    color={fileStore.orderBy === "name" ? mainColor() : undefined}
                    fontWeight={fileStore.orderBy === "name" ? "semibold" : "normal"}
                  >
                    File name
                  </Text>
                  <Show when={fileStore.orderBy === "name"}>
                    <Icon as={FiCheck} color={mainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
              <MenuItem cursor="pointer" onSelect={() => updateSort("modified", fileStore.reverse)}>
                <HStack w="$full" justifyContent="space-between" alignItems="center">
                  <Text
                    fontSize="$sm"
                    color={fileStore.orderBy === "modified" ? mainColor() : undefined}
                    fontWeight={fileStore.orderBy === "modified" ? "semibold" : "normal"}
                  >
                    Modified
                  </Text>
                  <Show when={fileStore.orderBy === "modified"}>
                    <Icon as={FiCheck} color={mainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
            </MenuGroup>
            <Divider my="$1" />
            <MenuGroup>
              <MenuLabel fontSize="$xs" color="$neutral9" px="$3" py="$1">
                Sort order
              </MenuLabel>
              <MenuItem cursor="pointer" onSelect={() => updateSort(fileStore.orderBy, false)}>
                <HStack w="$full" justifyContent="space-between" alignItems="center">
                  <Text
                    fontSize="$sm"
                    color={!fileStore.reverse ? mainColor() : undefined}
                    fontWeight={!fileStore.reverse ? "semibold" : "normal"}
                  >
                    A to Z
                  </Text>
                  <Show when={!fileStore.reverse}>
                    <Icon as={FiCheck} color={mainColor()} />
                  </Show>
                </HStack>
              </MenuItem>
              <MenuItem cursor="pointer" onSelect={() => updateSort(fileStore.orderBy, true)}>
                <HStack w="$full" justifyContent="space-between" alignItems="center">
                  <Text
                    fontSize="$sm"
                    color={fileStore.reverse ? mainColor() : undefined}
                    fontWeight={fileStore.reverse ? "semibold" : "normal"}
                  >
                    Z to A
                  </Text>
                  <Show when={fileStore.reverse}>
                    <Icon as={FiCheck} color={mainColor()} />
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
  const { pathname } = useRouter()
  const { loadFolder } = useFiles()

  return (
    <VStack class="list" w="$full" spacing="$0">
      <ListTitle
        sortCallback={(orderBy, reverse) => {
          FileStore.setSort(orderBy, reverse)
          void loadFolder(pathname(), 1, orderBy, reverse)
        }}
        initialOrder={fileStore.orderBy}
        initialReverse={fileStore.reverse}
      />
      <Show when={selectedMsg()}>
        <HStack
          w="$full"
          px={{ "@initial": "$3", "@md": "$4" }}
          py="$2"
          spacing="$3"
          borderBottom="1px solid"
          borderColor="$neutral4"
          bgColor={useColorModeValue("$info2", "$neutral4")()}
          flexWrap="wrap"
        >
          <Text size="sm" fontWeight="semibold" mr="auto">
            {selectedMsg()}
          </Text>
          <Show when={can("copy")}>
            <Button size="sm" variant="ghost" color={mainColor()} onClick={() => bus.emit("tool", "copy")}>
              Copy
            </Button>
          </Show>
          <Show when={can("move")}>
            <Button size="sm" variant="ghost" color={mainColor()} onClick={() => bus.emit("tool", "move")}>
              Move
            </Button>
          </Show>
          <Show when={can("delete")}>
            <Button size="sm" variant="ghost" color="$danger9" onClick={() => bus.emit("tool", "delete")}>
              Delete
            </Button>
          </Show>
          <Button size="sm" variant="ghost" color="$neutral10" onClick={() => selectAll(false)}>
            Cancel selection
          </Button>
        </HStack>
      </Show>
      <For each={visibleFileIndexes()}>
        {(index) => <ListItem obj={fileStore.files[index]} index={index} />}
      </For>
      <Show when={directoryFilter().trim() && visibleFileIndexes().length === 0}>
        <Text size="sm" color="$neutral11" p="$4">
          No matching files on this page
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
            ? `${visibleFileIndexes().length} matches on this page`
            : countMsg()}
        </Text>
      </HStack>
    </VStack>
  )
}

export default ListLayout
