import { Button, HStack, Text } from "@hope-ui/solid"
import { lazy, Show } from "solid-js"
import { useFiles, useRouter } from "~/hooks"
import { LIST_PAGE_SIZE, fileStore, selectAll } from "~/store"

const ListLayout = lazy(() => import("./List"))

const Pager = () => {
  const { pathname } = useRouter()
  const { loadFolder } = useFiles()
  const pageCount = () => Math.ceil(fileStore.total / LIST_PAGE_SIZE)
  const go = (page: number) => {
    selectAll(false)
    void loadFolder(pathname(), page)
  }

  return (
    <Show when={pageCount() > 1}>
      <HStack
        justifyContent="center"
        spacing="$3"
        py="$2"
        borderTop="1px solid"
        borderColor="$neutral4"
      >
        <Button
          size="sm"
          disabled={fileStore.page <= 1}
          onClick={() => go(fileStore.page - 1)}
        >
          Previous
        </Button>
        <Text size="sm">
          Page {fileStore.page} of {pageCount()} ({fileStore.total} items)
        </Text>
        <Button
          size="sm"
          disabled={fileStore.page >= pageCount()}
          onClick={() => go(fileStore.page + 1)}
        >
          Next
        </Button>
      </HStack>
    </Show>
  )
}

const Folder = () => (
  <>
    <ListLayout />
    <Pager />
  </>
)

export default Folder
