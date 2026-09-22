import {
  HStack,
  useColorModeValue,
  Image,
  Center,
  CenterProps,
} from "@hope-ui/solid"
import { Show, createMemo } from "solid-js"
import { getSetting, local, objStore, State } from "~/store"
import { Container } from "../Container"
import { joinBase } from "~/utils"
import { Layout } from "./layout"

export const Header = () => {
  const logos = getSetting("logo").split("\n")
  const logo = useColorModeValue(logos[0], logos.pop())

  const logoSrc = createMemo(() => {
    const value = logo() || "favicon.ico"
    if (/^(?:https?:)?\/\//.test(value) || /^(?:data|blob):/.test(value)) {
      return value
    }
    return joinBase(value)
  })

  const stickyProps = createMemo<CenterProps>(() => {
    switch (local["position_of_header_navbar"]) {
      case "sticky":
        return { position: "sticky", zIndex: "$sticky", top: 0 }
      default:
        return { position: undefined, zIndex: undefined, top: undefined }
    }
  })

  return (
    <Center
      {...stickyProps}
      bgColor="$background"
      class="header"
      w="$full"
      // shadow="$md"
    >
      <Container>
        <HStack
          px="calc(2% + 0.5rem)"
          py="$2"
          w="$full"
          justifyContent="space-between"
        >
          <HStack class="header-left" h="44px">
            <Image
              src={logoSrc()}
              h="$full"
              w="auto"
              fallback={<Image src={joinBase("favicon.ico")} h="$full" w="auto" />}
            />
          </HStack>
          <HStack class="header-right" spacing="$2">
            <Show when={objStore.state === State.Folder}>
              <Layout />
            </Show>
          </HStack>
        </HStack>
      </Container>
    </Center>
  )
}
