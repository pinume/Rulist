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
  objStore,
  setDirectoryFilter,
  State,
} from "~/store"
import { Container } from "../Container"
import { LinkWithBase } from "~/components"
import { authLogout, changeToken, handleResp, joinBase, notify } from "~/utils"
import { useRouter, useT } from "~/hooks"
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
          px={{ "@initial": "$3", "@md": "$6" }}
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
                w={{ "@initial": "150px", "@sm": "260px", "@md": "360px" }}
                size="sm"
              />
            </Show>
            <AddMenu />
            <Tooltip
              placement="bottom"
              withArrow
              label={t("global.logout") || "退出登录"}
            >
              <IconButton
                aria-label={t("global.logout") || "退出登录"}
                icon={
                  <svg
                    viewBox="0 0 24 24"
                    width="14"
                    height="14"
                    stroke="currentColor"
                    stroke-width="2.2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    fill="none"
                  >
                    <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
                    <polyline points="16 17 21 12 16 7" />
                    <line x1="21" y1="12" x2="9" y2="12" />
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
                  color: "$danger9",
                  borderColor: "$danger7",
                  bgColor: useColorModeValue("$danger2", "$danger4")(),
                  transform: "translateY(-1px)",
                  shadow: "$sm",
                }}
                _active={{
                  transform: "translateY(0)",
                  shadow: "none",
                }}
                onClick={async () => {
                  handleResp(await authLogout(), () => {
                    changeToken()
                    notify.success(t("global.logout_success") || "登出成功")
                    to("/@login?redirect=%2F")
                  })
                }}
              />
            </Tooltip>
          </HStack>
        </HStack>
      </Container>
    </Center>
  )
}
