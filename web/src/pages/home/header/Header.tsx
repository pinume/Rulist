import {
  HStack,
  useColorModeValue,
  Image,
  Center,
  Input,
  Icon,
  IconButton,
  Tooltip,
} from "@hope-ui/solid"
import {
  Show,
  createEffect,
  createMemo,
  on,
  onCleanup,
  onMount,
} from "solid-js"
import {
  clearDirectoryFilter,
  directoryFilter,
  getLogo,
  getMainColor,
  objStore,
  setDirectoryFilter,
  State,
} from "~/store"
import { Container } from "../Container"
import { LinkWithBase } from "~/components"
import { joinBase } from "~/utils"
import { useRouter, useT } from "~/hooks"
import { FiSettings } from "solid-icons/fi"
import { AddMenu } from "./AddMenu"

export const Header = () => {
  const t = useT()
  const { pathname, to } = useRouter()
  let searchInput: HTMLInputElement | undefined
  const [lightLogo, darkLogo] = getLogo()
  const logo = useColorModeValue(lightLogo, darkLogo)

  const logoSrc = createMemo(() => {
    const value = logo()
    if (/^(?:https?:)?\/\//.test(value) || /^(?:data|blob):/.test(value)) {
      return value
    }
    return joinBase(value)
  })

  createEffect(on(pathname, () => clearDirectoryFilter()))

  onMount(() => {
    const modalOpen = () =>
      !!document.querySelector('[role="dialog"], .hope-modal__content')
    const editable = (target: EventTarget | null) =>
      target instanceof Element &&
      !!target.closest('input, textarea, [contenteditable="true"]')
    const onKeyDown = (event: KeyboardEvent) => {
      if (objStore.state !== State.Folder || modalOpen()) return
      if (
        event.key === "/" &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !editable(event.target)
      ) {
        event.preventDefault()
        searchInput?.focus()
      }
      if (event.key !== "Escape") return
      if (document.activeElement === searchInput) {
        clearDirectoryFilter()
        searchInput?.blur()
      } else if (directoryFilter() && !editable(event.target)) {
        clearDirectoryFilter()
      }
    }
    window.addEventListener("keydown", onKeyDown)
    onCleanup(() => window.removeEventListener("keydown", onKeyDown))
  })

  return (
    <Center
      position="sticky"
      top={0}
      zIndex={100}
      bgColor="$background"
      h="60px"
      class="header"
      w="$full"
      // shadow="$md"
    >
      <Container>
        <HStack
          px="calc(2% + 0.5rem)"
          py="$2"
          w="$full"
          spacing="$2"
          justifyContent="space-between"
        >
          <HStack
            as={LinkWithBase}
            href="/"
            aria-label="返回首页"
            class="header-left"
            h="44px"
            w="44px"
            flexShrink={0}
          >
            <Image
              src={logoSrc()}
              h="32px"
              w="auto"
              fallback={
                <Image src={joinBase("favicon.ico")} h="32px" w="auto" />
              }
            />
          </HStack>
          <HStack spacing="$2" alignItems="center">
            <Show when={objStore.state === State.Folder}>
              <Input
                ref={searchInput}
                aria-label={t("home.search.aria_label")}
                placeholder={t("home.search.placeholder")}
                value={directoryFilter()}
                onInput={(event) =>
                  setDirectoryFilter(event.currentTarget.value)
                }
                w={{ "@initial": "150px", "@sm": "200px", "@md": "240px" }}
                size="sm"
              />
            </Show>
            <AddMenu />
            <Tooltip
              placement="bottom"
              withArrow
              label={t("home.toolbar.settings") || "设置"}
            >
              <IconButton
                aria-label={t("home.toolbar.settings") || "设置"}
                icon={
                  <svg
                    viewBox="0 0 24 24"
                    width="13"
                    height="13"
                    stroke="currentColor"
                    stroke-width="2.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    fill="none"
                    style={{
                      transition: "transform 0.3s cubic-bezier(0.4, 0, 0.2, 1)",
                    }}
                  >
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
                  </svg>
                }
                w="$7"
                h="$7"
                p={0}
                minW="unset"
                rounded="8px"
                cursor="pointer"
                bgColor={useColorModeValue("$neutral2", "$neutral4")()}
                color={useColorModeValue("#111827", "#f3f4f6")()}
                border="1px solid"
                borderColor={useColorModeValue("$neutral4", "$neutral6")()}
                shadow="$xs"
                transition="all 0.15s ease-in-out"
                _hover={{
                  color: getMainColor(),
                  borderColor: getMainColor(),
                  bgColor: useColorModeValue("$neutral3", "$neutral5")(),
                  transform: "translateY(-1px)",
                  shadow: "$sm",
                  "& svg": {
                    transform: "rotate(45deg)",
                  },
                }}
                _active={{
                  transform: "translateY(0)",
                  shadow: "none",
                }}
                onClick={() => to("/@settings")}
              />
            </Tooltip>
          </HStack>
        </HStack>
      </Container>
    </Center>
  )
}
