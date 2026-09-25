import { Button, HStack, Text } from "@hope-ui/solid"
import { lazy, Show } from "solid-js"
import { usePath, useRouter, useT } from "~/hooks"
import { LIST_PAGE_SIZE, objStore, selectAll } from "~/store"

const ListLayout = lazy(() => import("./List"))

const Pager = () => {
  const t = useT()
  const { pathname } = useRouter()
  const { handleFolder } = usePath()
  const pageCount = () => Math.ceil(objStore.total / LIST_PAGE_SIZE)
  const go = (page: number) => {
    selectAll(false)
    void handleFolder(pathname(), false, page)
  }

  return (
    <Show when={pageCount() > 1}>
      <HStack justifyContent="center" spacing="$3" py="$2" borderTop="1px solid" borderColor="$neutral4">
        <Button
          size="sm"
          disabled={objStore.page <= 1}
          onClick={() => go(objStore.page - 1)}
        >
          {t("home.pagination.previous")}
        </Button>
        <Text size="sm">
          {t("home.pagination.page", {
            page: objStore.page,
            pages: pageCount(),
            total: objStore.total,
          })}
        </Text>
        <Button
          size="sm"
          disabled={objStore.page >= pageCount()}
          onClick={() => go(objStore.page + 1)}
        >
          {t("home.pagination.next")}
        </Button>
      </HStack>
    </Show>
  )
}

const Folder = () => {
  return (
    <>
      <ListLayout />
      <Pager />
    </>
  )
}

export default Folder
