import { VStack } from "@hope-ui/solid"
import { Nav } from "./Nav"
import { Obj } from "./Obj"
import { Container } from "./Container"
import { DropZone } from "./DropZone"

export const Body = () => {
  return (
    <Container>
      <VStack
        class="body"
        mt="$2"
        py="$3"
        px={{ "@initial": "$3", "@md": "$6" }}
        minH="80vh"
        w="$full"
        gap="$3"
      >
        <Nav />
        <Obj />
      </VStack>
      <DropZone />
    </Container>
  )
}
