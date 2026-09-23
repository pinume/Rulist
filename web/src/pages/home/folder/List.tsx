import { HStack, VStack, Text } from "@hope-ui/solid"
import { For, Show } from "solid-js"
import { usePath, useT, useRouter } from "~/hooks"
import {
  allChecked,
  checkboxOpen,
  countMsg,
  isIndeterminate,
  saveSortState,
  ObjStore,
  OrderBy,
  objStore,
  selectAll,
  selectedMsg,
} from "~/store"
import { Col, cols, ListItem } from "./ListItem"
import { ItemCheckbox, useSelectWithMouse } from "./helper"
import { bus } from "~/utils"

export const ListTitle = (props: {
  sortCallback: (orderBy: OrderBy, reverse: boolean) => void
  disableCheckbox?: boolean
  initialOrder: OrderBy
  initialReverse: boolean
}) => {
  const t = useT()
  const { pathname } = useRouter()

  const updateSort = (nextOrder: OrderBy, nextReverse: boolean) => {
    saveSortState(pathname(), { orderBy: nextOrder, reverse: nextReverse })
    props.sortCallback(nextOrder, nextReverse)
  }

  const itemProps = (col: Col) => {
    return {
      fontWeight: "bold",
      fontSize: "$sm",
      color: "$neutral11",
      textAlign: col.textAlign as any,
      cursor: "pointer",
      onClick: () => {
        if (col.name === props.initialOrder) {
          updateSort(col.name, !props.initialReverse)
        } else {
          updateSort(col.name, false)
        }
      },
    }
  }
  return (
    <HStack class="title" w="$full" p="$2">
      <HStack w={cols[0].w} spacing="$1">
        <Show when={!props.disableCheckbox && checkboxOpen()}>
          <ItemCheckbox
            checked={allChecked()}
            indeterminate={isIndeterminate()}
            onChange={(e: any) => {
              selectAll(e.target.checked as boolean)
            }}
          />
        </Show>
        {selectedMsg() ? (
          <Text {...itemProps(cols[0])}>{selectedMsg()}</Text>
        ) : (
          <Text {...itemProps(cols[0])}>{t(`home.obj.${cols[0].name}`)}</Text>
        )}
      </HStack>
      <Text w={cols[1].w} {...itemProps(cols[1])}>
        {t(`home.obj.${cols[1].name}`)}
      </Text>
      <Text
        w={cols[2].w}
        {...itemProps(cols[2])}
        display={{ "@initial": "none", "@md": "inline" }}
      >
        {t(`home.obj.${cols[2].name}`)}
      </Text>
    </HStack>
  )
}

const ListLayout = () => {
  const { pathname } = useRouter()
  const { handleFolder } = usePath()

  const { registerSelectContainer, captureContentMenu } = useSelectWithMouse()
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
      oncapture:contextmenu={captureContentMenu}
      class="list viselect-container"
      w="$full"
      spacing="$1"
    >
      <ListTitle
        sortCallback={(orderBy, reverse) => {
          ObjStore.setSort(orderBy, reverse)
          void handleFolder(pathname(), false, 1, orderBy, reverse)
        }}
        initialOrder={objStore.orderBy}
        initialReverse={objStore.reverse}
      />
      <For each={objStore.objs}>
        {(obj, i) => {
          return <ListItem obj={obj} index={i()} />
        }}
      </For>
      <Text size="sm" color="$neutral11">
        {countMsg()}
      </Text>
    </VStack>
  )
}

export default ListLayout
