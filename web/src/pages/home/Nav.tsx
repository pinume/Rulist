import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbSeparator,
  HStack,
} from "@hope-ui/solid"
import { Link } from "@solidjs/router"
import { createMemo, For, Show } from "solid-js"
import { usePath, useRouter } from "~/hooks"
import { encodePath, joinBase } from "~/utils"

export const Nav = () => {
  const { pathname } = useRouter()
  const paths = createMemo(() => {
    return pathname().split("/").filter(Boolean)
  })
  const { setPathAs } = usePath()

  return (
    <Show when={paths().length > 0}>
      <HStack
        position="sticky"
        top="60px"
        zIndex={90}
        minH="40px"
        py="$1"
        background="$background"
        _after={{
          content: '""',
          backgroundColor: "$background",
          position: "absolute",
          height: "100%",
          width: "99vw",
          zIndex: -1,
          transform: "translateX(-50%)",
          left: "50%",
          top: 0,
        }}
        w="$full"
      >
        <Breadcrumb class="nav" minW={0} overflow="hidden">
          <BreadcrumbItem class="nav-item">
            <BreadcrumbLink
              class="nav-link"
              as={Link}
              href={joinBase("/")}
              color="$neutral10"
              _hover={{ color: "$neutral12" }}
              cursor="pointer"
              px="$1"
              py="$0_5"
              onMouseEnter={() => setPathAs("/")}
            >
              我的文件
            </BreadcrumbLink>
            <BreadcrumbSeparator class="nav-separator" />
          </BreadcrumbItem>
          <For each={paths()}>
            {(name, i) => {
              const isLast = createMemo(() => i() === paths().length - 1)
              const path = `/${paths()
                .slice(0, i() + 1)
                .join("/")}`
              const href = encodePath(path)
              return (
                <BreadcrumbItem class="nav-item">
                  <BreadcrumbLink
                    class="nav-link"
                    css={{
                      wordBreak: "break-all",
                    }}
                    color={isLast() ? "$neutral12" : "$neutral10"}
                    _hover={{ color: "$neutral12" }}
                    cursor="pointer"
                    px="$1"
                    py="$0_5"
                    currentPage={isLast()}
                    as={isLast() ? undefined : Link}
                    href={joinBase(href)}
                    onMouseEnter={() => setPathAs(path)}
                  >
                    {name}
                  </BreadcrumbLink>
                  <Show when={!isLast()}>
                    <BreadcrumbSeparator class="nav-separator" />
                  </Show>
                </BreadcrumbItem>
              )
            }}
          </For>
        </Breadcrumb>
      </HStack>
    </Show>
  )
}
