import { Button, Text, VStack } from "@hope-ui/solid"
import { useT } from "~/hooks"
import { objStore } from "~/store"
import { getFileSize } from "~/utils"

const File = () => {
  const t = useT()
  const startDownload = () => {
    const url = new URL(objStore.raw_url, window.location.href)
    url.searchParams.set("openlist_ts", Date.now().toString())
    const anchor = document.createElement("a")
    anchor.href = url.toString()
    anchor.download = objStore.obj.name
    anchor.click()
  }
  return (
    <VStack w="$full" minH="20vh" justifyContent="center" spacing="$3">
      <Text>{objStore.obj.name}</Text>
      <Text color="$neutral10">{getFileSize(objStore.obj.size)}</Text>
      <Button onClick={startDownload}>
        {t("home.toolbar.download")}
      </Button>
    </VStack>
  )
}

export default File
