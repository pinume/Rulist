import {
  Icon,
  Menu,
  MenuContent,
  MenuItem,
  MenuTrigger,
  Text,
  Tooltip,
  useColorModeValue,
} from "@hope-ui/solid"
import { FiPlus } from "solid-icons/fi"
import {
  RiDocumentFileUploadLine,
  RiDocumentFolderUploadLine,
  RiDocumentFolderAddLine,
} from "solid-icons/ri"
import { createMemo, Show } from "solid-js"
import { useT } from "~/hooks"
import { getMainColor, objStore, State, userCan } from "~/store"
import { bus } from "~/utils"
import { enqueueFilesForUpload } from "../uploads/util"

export const AddMenu = () => {
  const t = useT()

  const canWrite = createMemo(
    () =>
      objStore.state === State.Folder &&
      (userCan("write_content") || objStore.write_content_bypass) &&
      objStore.write,
  )

  let fileInputRef: HTMLInputElement | undefined
  let folderInputRef: HTMLInputElement | undefined

  const handleUploadFiles = () => {
    fileInputRef?.click()
  }

  const handleUploadFolder = () => {
    folderInputRef?.click()
  }

  const handleCreateFolder = () => {
    bus.emit("tool", "mkdir")
  }

  return (
    <Show when={canWrite()}>
      <input
        type="file"
        multiple
        ref={fileInputRef}
        style={{ display: "none" }}
        onChange={(e) => {
          const files = Array.from(e.currentTarget.files ?? [])
          e.currentTarget.value = ""
          enqueueFilesForUpload(files)
        }}
      />
      <input
        type="file"
        multiple
        // @ts-ignore
        webkitdirectory=""
        ref={folderInputRef}
        style={{ display: "none" }}
        onChange={(e) => {
          const files = Array.from(e.currentTarget.files ?? [])
          e.currentTarget.value = ""
          enqueueFilesForUpload(files)
        }}
      />

      <Menu placement="bottom-end" offset={6}>
        <Tooltip
          placement="bottom"
          withArrow
          label={t("home.add_menu.title") || "新建与上传"}
        >
          <MenuTrigger
            aria-label={t("home.add_menu.title") || "新建与上传"}
            w="$7"
            h="$7"
            p={0}
            minW="unset"
            rounded="8px"
            cursor="pointer"
            color={useColorModeValue("#111827", "#f3f4f6")()}
            bgColor={useColorModeValue("$neutral2", "$neutral4")()}
            border="1px solid"
            borderColor={useColorModeValue("$neutral4", "$neutral6")()}
            shadow="$xs"
            transition="all 0.15s ease-in-out"
            display="inline-flex"
            alignItems="center"
            justifyContent="center"
            _hover={{
              color: getMainColor(),
              borderColor: getMainColor(),
              bgColor: useColorModeValue("$neutral3", "$neutral5")(),
              transform: "translateY(-1px)",
              shadow: "$sm",
            }}
            _active={{
              transform: "translateY(0)",
              shadow: "none",
            }}
          >
            <svg
              viewBox="0 0 24 24"
              width="13"
              height="13"
              stroke="currentColor"
              stroke-width="2.6"
              stroke-linecap="round"
              stroke-linejoin="round"
              fill="none"
            >
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
          </MenuTrigger>
        </Tooltip>

        <MenuContent
          minW="165px"
          p="$1_5"
          rounded="$lg"
          shadow="$lg"
          border="1px solid"
          borderColor={useColorModeValue("$neutral4", "$neutral6")()}
          bgColor={useColorModeValue("$neutral1", "$neutral3")()}
          zIndex={110}
        >
          <MenuItem
            cursor="pointer"
            icon={
              <Icon
                as={RiDocumentFileUploadLine}
                boxSize="$4"
                color={getMainColor()}
              />
            }
            onSelect={handleUploadFiles}
            rounded="$md"
            py="$2"
            px="$2_5"
            _hover={{
              bgColor: useColorModeValue("$neutral3", "$neutral5")(),
            }}
          >
            <Text fontSize="$sm" fontWeight="$medium">
              {t("home.add_menu.upload_file") || "上传文件"}
            </Text>
          </MenuItem>

          <MenuItem
            cursor="pointer"
            icon={
              <Icon
                as={RiDocumentFolderUploadLine}
                boxSize="$4"
                color={getMainColor()}
              />
            }
            onSelect={handleUploadFolder}
            rounded="$md"
            py="$2"
            px="$2_5"
            _hover={{
              bgColor: useColorModeValue("$neutral3", "$neutral5")(),
            }}
          >
            <Text fontSize="$sm" fontWeight="$medium">
              {t("home.add_menu.upload_folder") || "上传文件夹"}
            </Text>
          </MenuItem>

          <MenuItem
            cursor="pointer"
            icon={
              <Icon
                as={RiDocumentFolderAddLine}
                boxSize="$4"
                color={getMainColor()}
              />
            }
            onSelect={handleCreateFolder}
            rounded="$md"
            py="$2"
            px="$2_5"
            _hover={{
              bgColor: useColorModeValue("$neutral3", "$neutral5")(),
            }}
          >
            <Text fontSize="$sm" fontWeight="$medium">
              {t("home.add_menu.create_folder") || "创建文件夹"}
            </Text>
          </MenuItem>
        </MenuContent>
      </Menu>
    </Show>
  )
}
