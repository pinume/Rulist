import { Box, HStack, IconButton } from "@hope-ui/solid"
import { FiMenu, FiX } from "solid-icons/fi"
import { useLocation } from "@solidjs/router"
import {
  Show,
  createEffect,
  createMemo,
  createSignal,
  on,
  onCleanup,
  onMount,
} from "solid-js"
import { FolderTree, FolderTreeHandler } from "~/components"
import { useRouter, useT } from "~/hooks"
import { local, objStore } from "~/store"
import { objBoxRef } from "./Obj"

function SidebarPanel() {
  const { to } = useRouter()
  const t = useT()
  const location = useLocation()

  const [folderTreeHandler, setFolderTreeHandler] =
    createSignal<FolderTreeHandler>()
  const [sideBarRef, setSideBarRef] = createSignal<HTMLDivElement>()
  const [offsetX, setOffsetX] = createSignal<number | string>(-999)
  const [expanded, setExpanded] = createSignal(false)

  const showFullSidebar = () => setOffsetX(0)
  const resetSidebar = () => {
    if (expanded()) return showFullSidebar()
    const $objBox = objBoxRef()
    const $sideBar = sideBarRef()
    if (!$objBox || !$sideBar) return
    const gap = $objBox.offsetLeft > 50 ? 16 : 0
    if ($sideBar.clientWidth < $objBox.offsetLeft - gap) {
      setOffsetX(0)
    } else {
      setOffsetX(`calc(-100% + ${$objBox.offsetLeft}px - ${gap}px)`)
    }
  }
  const closeSidebar = () => {
    setExpanded(false)
    resetSidebar()
    requestAnimationFrame(() =>
      document.getElementById("sidebar-open")?.focus(),
    )
  }

  let rafId: number

  onMount(() => {
    const handler = folderTreeHandler()
    handler?.setPath(location.pathname)
    rafId = requestAnimationFrame(resetSidebar)
    window.addEventListener("resize", resetSidebar)
    onCleanup(() => window.removeEventListener("resize", resetSidebar))
  })

  createEffect(
    on(
      () => objStore.state,
      () => {
        cancelAnimationFrame(rafId)
        rafId = requestAnimationFrame(resetSidebar)
      },
    ),
  )

  createEffect(
    on(
      () => location.pathname,
      () => {
        const handler = folderTreeHandler()
        handler?.setPath(location.pathname)
      },
    ),
  )

  return (
    <>
      <Show when={offsetX() !== 0}>
        <IconButton
          id="sidebar-open"
          type="button"
          aria-label={t("home.sidebar.open")}
          aria-controls="home-sidebar"
          aria-expanded="false"
          icon={<FiMenu />}
          pos="fixed"
          left="$2"
          top="$16"
          zIndex="$overlay"
          onClick={() => {
            setExpanded(true)
            showFullSidebar()
          }}
        />
      </Show>
      <Box
        id="home-sidebar"
        transform={`translateX(${typeof offsetX() === "number" ? `${offsetX()}px` : offsetX()})`}
        transition="transform 0.2s"
        zIndex="$overlay"
        pos="fixed"
        left={3} // width of outline shadow
        top={3}
        h="calc(100vh - 6px)"
        minW={180}
        p="$2"
        overflow="auto"
        shadow="$lg"
        rounded="$lg"
        bgColor="white"
        _dark={{ bgColor: "$neutral3" }}
        onMouseEnter={showFullSidebar}
        onMouseLeave={resetSidebar}
        onFocusIn={showFullSidebar}
        onFocusOut={(event: FocusEvent) => {
          if (!sideBarRef()?.contains(event.relatedTarget as Node))
            resetSidebar()
        }}
        ref={(el: HTMLDivElement) => setSideBarRef(el)}
      >
        <Show when={expanded()}>
          <HStack justifyContent="flex-end" mb="$2">
            <IconButton
              type="button"
              aria-label={t("home.sidebar.close")}
              icon={<FiX />}
              onClick={closeSidebar}
            />
          </HStack>
        </Show>
        <FolderTree
          autoOpen
          showEmptyIcon
          showHiddenFolder={false}
          onChange={(path) => {
            setExpanded(false)
            to(path)
            requestAnimationFrame(resetSidebar)
          }}
          handle={(handler) => setFolderTreeHandler(handler)}
        />
      </Box>
    </>
  )
}

export function Sidebar() {
  const visible = createMemo(() => local["show_sidebar"] !== "none")

  return (
    <Show when={visible()}>
      <SidebarPanel />
    </Show>
  )
}
