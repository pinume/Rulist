import {
  HStack,
  Icon,
  Menu,
  MenuContent,
  MenuItem,
  MenuTrigger,
  Text,
  useColorModeValue,
} from "@hope-ui/solid"
import { Show } from "solid-js"
import { LinkWithPush } from "~/components"
import { useLink, usePath, useRouter, useT, useUtil } from "~/hooks"
import {
  getMainColor,
  getSettingBool,
  me,
  objStore,
  OrderBy,
  selectIndex,
  userCan,
} from "~/store"
import { StoreObj, UserMethods } from "~/types"
import {
  bus,
  colorAlpha,
  formatDate,
  getFileSize,
  hoverColor,
  notify,
} from "~/utils"
import { getIconByObj, getIconColorByObj } from "~/utils/icon"
import { BsThreeDotsVertical } from "solid-icons/bs"
import { operations } from "../toolbar/operations"

export interface Col {
  name: string
  textAlign: "left" | "right"
  w: any
  minW?: any
}

export const cols: Col[] = [
  {
    name: "name",
    textAlign: "left",
    w: { "@initial": "calc(100% - 50px)", "@md": "45%" },
  },
  { name: "size", textAlign: "right", w: { "@initial": 0, "@md": "20%" } },
  { name: "modified", textAlign: "right", w: { "@initial": 0, "@md": "23%" } },
  {
    name: "actions",
    textAlign: "right",
    w: { "@initial": "50px", "@md": "12%" },
    minW: { "@initial": "40px", "@md": "70px" },
  },
]

export const ListItem = (props: { obj: StoreObj; index: number }) => {
  const { isHide } = useUtil()
  if (isHide(props.obj)) {
    return null
  }
  const t = useT()
  const { rawLink } = useLink()
  const { setPathAs } = usePath()
  const { pushHref, to } = useRouter()
  const hasAnyAction = () => {
    if (!props.obj.is_dir) return true
    if (getSettingBool("package_download")) return true
    if (objStore.write) {
      if (
        userCan("rename") ||
        userCan("copy") ||
        userCan("move") ||
        userCan("delete")
      ) {
        return true
      }
    }
    return false
  }
  return (
    <div
      style={{
        width: "100%",
      }}
    >
      <HStack
        classList={{ selected: !!props.obj.selected }}
        class="list-item viselect-item"
        data-index={props.index}
        w="$full"
        px={{ "@initial": "$3", "@md": "$4" }}
        py="$3"
        borderBottom="1px solid"
        borderColor="$neutral3"
        transition="background-color 0.15s"
        _hover={{
          bgColor: props.obj.selected
            ? colorAlpha(getMainColor(), 0.13)
            : hoverColor(),
        }}
        cursor="pointer"
        bgColor={
          props.obj.selected ? colorAlpha(getMainColor(), 0.13) : undefined
        }
        onClick={(e: MouseEvent) => {
          const target = e.target as HTMLElement | null
          if (
            target?.closest(
              ".actions, .hope-menu__trigger, .hope-menu__content, .hope-menu__item",
            )
          ) {
            return
          }
          e.preventDefault()
          to(pushHref(props.obj.name))
        }}
        onMouseEnter={() => {
          setPathAs(props.obj.name, props.obj.is_dir, true)
        }}
      >
        <HStack class="name-box" spacing="$1" w={cols[0].w}>
          <Icon
            class="icon"
            boxSize="$6"
            color={getIconColorByObj(props.obj)}
            as={getIconByObj(props.obj)}
            mr="$1"
          />
          <LinkWithPush
            href={props.obj.name}
            style={{
              "min-width": "0",
              flex: "1",
              "text-decoration": "none",
              color: "inherit",
              display: "flex",
              "align-items": "center",
            }}
          >
            <HStack
              flexDirection={{ "@initial": "column", "@md": "row" }}
              alignItems={{ "@initial": "flex-start", "@md": "center" }}
              spacing="$0"
              minW="0"
              flex="1"
            >
              <Text
                class="name"
                css={{
                  wordBreak: "break-all",
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
                title={props.obj.name}
                minW="0"
              >
                {props.obj.name}
              </Text>
              <Text
                display={{ "@initial": "block", "@md": "none" }}
                size="xs"
                color="$neutral10"
              >
                {getFileSize(props.obj.size)} · {formatDate(props.obj.modified)}
              </Text>
            </HStack>
          </LinkWithPush>
        </HStack>
        <HStack
          class="size"
          w={cols[1].w}
          display={{ "@initial": "none", "@md": "flex" }}
          justifyContent="flex-end"
          spacing="$3"
        >
          <Show when={props.obj.permissions}>
            <Text color="$neutral10" size="sm" fontFamily="inherit">
              {props.obj.permissions}
            </Text>
          </Show>
          <Text textAlign="right" size="sm">
            {getFileSize(props.obj.size)}
          </Text>
        </HStack>
        <Text
          class="modified"
          display={{ "@initial": "none", "@md": "inline" }}
          w={cols[2].w}
          textAlign={cols[2].textAlign as any}
        >
          {formatDate(props.obj.modified)}
        </Text>
        <HStack
          class="actions"
          w={cols[3].w}
          minW={cols[3].minW}
          justifyContent="flex-end"
          flexShrink={0}
        >
          <Show when={hasAnyAction()}>
            <Menu placement="bottom-end">
              <MenuTrigger
                px="$1"
                py="$1"
                h="auto"
                minW="unset"
                rounded="$md"
                cursor="pointer"
                bg="transparent"
                color="$neutral10"
                _hover={{
                  bgColor: useColorModeValue("$neutral3", "$neutral5")(),
                  color: "$neutral12",
                }}
                aria-label="Actions"
              >
                <Icon as={BsThreeDotsVertical} boxSize="$4" />
              </MenuTrigger>
              <MenuContent shadow="$md" zIndex={100}>
                <Show when={!props.obj.is_dir || getSettingBool("package_download")}>
                  <MenuItem
                    cursor="pointer"
                    icon={
                      <Icon
                        as={operations.download.icon}
                        color={operations.download.color}
                      />
                    }
                    onSelect={() => {
                      if (props.obj.is_dir) {
                        selectIndex(props.index, true, true)
                        bus.emit("tool", "package_download")
                      } else {
                        const url = rawLink(props.obj, true)
                        if (url) window.open(url, "_blank")
                      }
                    }}
                  >
                    {t("home.toolbar.download")}
                  </MenuItem>
                </Show>
                <Show when={userCan("rename") && objStore.write}>
                  <MenuItem
                    cursor="pointer"
                    icon={
                      <Icon
                        as={operations.rename.icon}
                        color={operations.rename.color}
                      />
                    }
                    onSelect={() => {
                      selectIndex(props.index, true, true)
                      bus.emit("tool", "rename")
                    }}
                  >
                    {t("home.toolbar.rename")}
                  </MenuItem>
                </Show>
                <Show when={userCan("copy") && objStore.write}>
                  <MenuItem
                    cursor="pointer"
                    icon={
                      <Icon
                        as={operations.copy.icon}
                        color={operations.copy.color}
                      />
                    }
                    onSelect={() => {
                      selectIndex(props.index, true, true)
                      bus.emit("tool", "copy")
                    }}
                  >
                    {t("home.toolbar.copy")}
                  </MenuItem>
                </Show>
                <Show when={userCan("move") && objStore.write}>
                  <MenuItem
                    cursor="pointer"
                    icon={
                      <Icon
                        as={operations.move.icon}
                        color={operations.move.color}
                      />
                    }
                    onSelect={() => {
                      selectIndex(props.index, true, true)
                      bus.emit("tool", "move")
                    }}
                  >
                    {t("home.toolbar.move")}
                  </MenuItem>
                </Show>
                <Show when={userCan("delete") && objStore.write}>
                  <MenuItem
                    cursor="pointer"
                    icon={
                      <Icon
                        as={operations.delete.icon}
                        color={operations.delete.color}
                      />
                    }
                    onSelect={() => {
                      selectIndex(props.index, true, true)
                      bus.emit("tool", "delete")
                    }}
                  >
                    {t("home.toolbar.delete")}
                  </MenuItem>
                </Show>
              </MenuContent>
            </Menu>
          </Show>
        </HStack>
      </HStack>
    </div>
  )
}
