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
            <Tooltip
              placement="bottom"
              withArrow
              label={t("home.toolbar.settings") || "设置"}
            >
              <IconButton
                aria-label={t("home.toolbar.settings") || "设置"}
                icon={<Icon as={FiSettings} boxSize="$5" />}
                size="sm"
                variant="ghost"
                colorScheme="neutral"
                color="$neutral11"
                _hover={{
                  color: getMainColor(),
                  bgColor: useColorModeValue("$neutral3", "$neutral5")(),
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
