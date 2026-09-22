import {
  IconButton,
  Menu,
  MenuContent,
  MenuItem,
  MenuTrigger,
} from "@hope-ui/solid"
import { BsGridFill } from "solid-icons/bs"
import { FaSolidListUl } from "solid-icons/fa"
import { Switch, Match, For } from "solid-js"
import { Dynamic } from "solid-js/web"
import { useT } from "~/hooks"
import { getMainColor, LayoutType, layout, setLayout } from "~/store"
import { colorAlpha } from "~/utils"

const layouts = {
  list: FaSolidListUl,
  grid: BsGridFill,
} as const

export const Layout = () => {
  const t = useT()
  return (
    <Menu>
      <MenuTrigger
        as={IconButton}
        color={getMainColor()}
        bgColor={colorAlpha(getMainColor(), 0.15)}
        _hover={{
          bgColor: colorAlpha(getMainColor(), 0.2),
        }}
        aria-label="switch layout"
        compact
        size="lg"
        icon={
          <Switch>
            <Match when={layout() === "list"}>
              <FaSolidListUl />
            </Match>
            <Match when={layout() === "grid"}>
              <BsGridFill />
            </Match>
          </Switch>
        }
      ></MenuTrigger>
      <MenuContent>
        <For each={Object.entries(layouts)}>
          {(item) => (
            <MenuItem
              icon={<Dynamic component={item[1]} />}
              onSelect={() => {
                setLayout(item[0] as LayoutType)
              }}
            >
              {t(`home.layouts.${item[0]}`)}
            </MenuItem>
          )}
        </For>
      </MenuContent>
    </Menu>
  )
}
