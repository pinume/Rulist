import { Menu, Item, Submenu } from "solid-contextmenu"
import { useSelectedLink, useT } from "~/hooks"
import "solid-contextmenu/dist/style.css"
import { HStack, Icon, Text, useColorMode } from "@hope-ui/solid"
import { operations } from "../toolbar/operations"
import { For, Show } from "solid-js"
import { bus, notify } from "~/utils"
import { UserMethods } from "~/types"
import {
  getSettingBool,
  haveSelected,
  me,
  objStore,
  oneChecked,
  selectedObjs,
  userCan,
} from "~/store"

const ItemContent = (props: { name: string }) => {
  const t = useT()
  return (
    <HStack spacing="$2">
      <Icon
        p={operations[props.name].p ? "$1" : undefined}
        as={operations[props.name].icon}
        boxSize="$7"
        color={operations[props.name].color}
      />
      <Text>{t(`home.toolbar.${props.name}`)}</Text>
    </HStack>
  )
}

export const ContextMenu = () => {
  const t = useT()
  const { colorMode } = useColorMode()
  const { rawLinks } = useSelectedLink()
  const canPackageDownload = () => {
    return UserMethods.is_admin(me()) || getSettingBool("package_download")
  }
  return (
    <Menu
      id={1}
      animation="scale"
      theme={colorMode() !== "dark" ? "light" : "dark"}
      style="z-index: var(--hope-zIndices-popover)"
    >
      <For each={["rename", "move", "copy", "delete"] as const}>
        {(name) => (
          <Item
            hidden={!userCan(name) || !objStore.write}
            onClick={() => {
              bus.emit("tool", name)
            }}
          >
            <ItemContent name={name} />
          </Item>
        )}
      </For>
      <Show when={oneChecked()}>
        <Item
          onClick={({ props }) => {
            if (props.is_dir) {
              if (!canPackageDownload()) {
                notify.warning(t("home.toolbar.package_download_disabled"))
                return
              }
              bus.emit("tool", "package_download")
            } else {
              const url = rawLinks(true)[0]
              if (url) window.open(url, "_blank")
            }
          }}
        >
          <ItemContent name="download" />
        </Item>
      </Show>
      <Show when={!oneChecked() && haveSelected()}>
        <Show when={canPackageDownload()}>
          <Submenu label={<ItemContent name="download" />}>
            <Item onClick={() => bus.emit("tool", "package_download")}>
              {t("home.toolbar.package_download")}
            </Item>
          </Submenu>
        </Show>
      </Show>
    </Menu>
  )
}
