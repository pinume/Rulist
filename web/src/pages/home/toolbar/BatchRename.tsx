import {
  Button,
  createDisclosure,
  HStack,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
  ModalOverlay,
  Text,
  VStack,
  Radio,
  RadioGroup,
  Input,
} from "@hope-ui/solid"
import { useFetch, useFiles, useRouter } from "~/hooks"
import {
  bus,
  fsBatchRename,
  handleRespWithNotifySuccess,
  hoverColor,
  notify,
  validateFilename,
} from "~/utils"
import { createSignal, For, onCleanup, Show } from "solid-js"
import { selectedFiles } from "~/store"
import { RenameEntry } from "~/types"

const validationMessage = (error?: string) =>
  error === "invalid_filename_chars"
    ? 'File names cannot contain: / \\ ? < > * : | "'
    : "Please enter a value"

const RenameItem = (props: { obj: RenameEntry; index: number }) => (
  <div style={{ width: "100%" }}>
    <HStack
      class="list-item"
      w="$full"
      p="$2"
      rounded="$lg"
      transition="all 0.3s"
      _hover={{ transform: "scale(1.01)", bgColor: hoverColor() }}
    >
      <Text
        w={{ "@initial": "50%", "@md": "50%" }}
        class="name"
        css={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}
        title={props.obj.src_name}
      >
        {props.obj.src_name}
      </Text>
      <Text
        w={{ "@initial": "50%", "@md": "50%" }}
        class="name"
        css={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}
        title={props.obj.new_name}
      >
        {props.obj.new_name}
      </Text>
    </HStack>
  </div>
)

export const BatchRename = () => {
  const {
    isOpen: isPreviewModalOpen,
    onOpen: openPreviewModal,
    onClose: closePreviewModal,
  } = createDisclosure()
  const { isOpen, onOpen, onClose } = createDisclosure()
  const [loading, ok] = useFetch(fsBatchRename)
  const { pathname } = useRouter()
  const { refresh } = useFiles()
  const [type, setType] = createSignal("3")
  const [srcName, setSrcName] = createSignal("")
  const [newName, setNewName] = createSignal("")
  const [paddingZeros, setPaddingZeros] = createSignal("")
  const [matchNames, setMatchNames] = createSignal<RenameEntry[]>([])
  const [validationErrorSrc, setValidationErrorSrc] = createSignal<string>("")
  const [validationErrorNew, setValidationErrorNew] = createSignal<string>("")

  const handleInputSrc = (newValue: string) => {
    setSrcName(newValue)
    const validation = validateFilename(newValue)
    setValidationErrorSrc(validation.valid ? "" : validation.error || "")
  }

  const handleInputNew = (newValue: string) => {
    setNewName(newValue)
    const validation = validateFilename(newValue)
    setValidationErrorNew(validation.valid ? "" : validation.error || "")
  }

  const itemProps = () => ({
    fontWeight: "bold",
    fontSize: "$sm",
    color: "$neutral11",
    textAlign: "left" as any,
    cursor: "pointer",
  })

  const handler = (name: string) => {
    if (name === "batchRename") onOpen()
  }
  bus.on("tool", handler)
  onCleanup(() => bus.off("tool", handler))

  const submit = () => {
    if (!srcName()) {
      notify.warning("Please enter a value")
      return
    }
    const validationSrc = validateFilename(srcName())
    if (!validationSrc.valid) {
      notify.warning(validationMessage(validationSrc.error))
      return
    }
    const validationNew = validateFilename(newName())
    if (!validationNew.valid) {
      notify.warning(validationMessage(validationNew.error))
      return
    }

    let matches: RenameEntry[]
    if (type() === "2") {
      let tempNum = newName()
      const hasNumberPlaceholder = srcName().includes("{number}")
      const paddingLength = parseInt(paddingZeros()) || 0
      matches = selectedFiles().map((obj) => {
        const lastDotIndex = obj.name.lastIndexOf(".")
        const suffix = lastDotIndex !== -1 ? obj.name.substring(lastDotIndex) : ""
        const paddedNum = paddingLength > 0 ? tempNum.padStart(paddingLength, "0") : tempNum
        const newFileName = hasNumberPlaceholder
          ? srcName().replace("{number}", paddedNum) + suffix
          : srcName() + paddedNum + suffix
        tempNum = (parseInt(tempNum) + 1).toString().padStart(tempNum.length, "0")
        return { src_name: obj.name, new_name: newFileName }
      })
    } else {
      matches = selectedFiles()
        .filter((obj) => obj.name.includes(srcName()))
        .map((obj) => ({
          src_name: obj.name,
          new_name: obj.name.replace(srcName(), newName()),
        }))
    }

    setMatchNames(matches)
    openPreviewModal()
    onClose()
  }

  const reset = () => {
    setType("3")
    setPaddingZeros("")
    setValidationErrorSrc("")
    setValidationErrorNew("")
  }

  return (
    <>
      <Modal
        blockScrollOnMount={false}
        opened={isOpen()}
        onClose={onClose}
        initialFocus="#modal-input1"
        size={{ "@initial": "xs", "@md": "md" }}
      >
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>Batch rename</ModalHeader>
          <ModalBody>
            <RadioGroup
              value={type()}
              onChange={(event: string) => {
                setType(event)
                setNewName("")
                setValidationErrorSrc("")
                setValidationErrorNew("")
              }}
            >
              <HStack spacing="$4">
                <Radio value="3">Find and replace</Radio>
                <Radio value="2">Sequential rename</Radio>
              </HStack>
            </RadioGroup>
            <VStack spacing="$2">
              <p style={{ margin: "10px 0" }}>
                <Show when={type() === "2"}>
                  Append a sequence number to new file names. Enter the new file name first and the starting number second. The {"{number}"} placeholder is supported.
                </Show>
                <Show when={type() === "3"}>
                  Find text in the selected file names and replace it.
                </Show>
              </p>
              <Input
                id="modal-input1"
                type="string"
                placeholder={type() === "2" ? "New file name (supports {number})" : "Find"}
                value={srcName()}
                invalid={!!validationErrorSrc()}
                onInput={(e) => handleInputSrc(e.currentTarget.value)}
                onKeyDown={(e) => { if (e.key === "Enter") submit() }}
              />
              <Show when={validationErrorSrc()}>
                <Text color="$danger9" fontSize="$sm">
                  {validationMessage(validationErrorSrc())}
                </Text>
              </Show>
              <Input
                id="modal-input2"
                type={type() === "2" ? "number" : "text"}
                placeholder={type() === "2" ? "Starting number (for example, 1)" : "Replace with"}
                value={newName()}
                invalid={!!validationErrorNew()}
                onInput={(e) => handleInputNew(e.currentTarget.value)}
                onKeyDown={(e) => { if (e.key === "Enter") submit() }}
              />
              <Show when={validationErrorNew()}>
                <Text color="$danger9" fontSize="$sm">
                  {validationMessage(validationErrorNew())}
                </Text>
              </Show>
              <Show when={type() === "2"}>
                <Input
                  id="modal-input3"
                  type="number"
                  min="0"
                  step="1"
                  placeholder="Zero-padding width (optional)"
                  value={paddingZeros()}
                  onInput={(e) => setPaddingZeros(e.currentTarget.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") submit() }}
                />
              </Show>
            </VStack>
          </ModalBody>
          <ModalFooter display="flex" gap="$2">
            <Button
              onClick={() => {
                reset()
                onClose()
              }}
              colorScheme="neutral"
            >
              Cancel
            </Button>
            <Button
              onClick={submit}
              disabled={!srcName() || !newName() || !!validationErrorSrc() || !!validationErrorNew()}
            >
              OK
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>

      <Modal size="xl" opened={isPreviewModalOpen()} onClose={closePreviewModal}>
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>Renamed files</ModalHeader>
          <ModalBody>
            <VStack class="list" w="$full" spacing="$1">
              <HStack class="title" w="$full" p="$2">
                <Text w={{ "@initial": "50%", "@md": "50%" }} {...itemProps()}>Old name</Text>
                <Text w={{ "@initial": "50%", "@md": "50%" }} {...itemProps()}>New name</Text>
              </HStack>
              <For each={matchNames()}>{(obj, i) => <RenameItem obj={obj} index={i()} />}</For>
            </VStack>
          </ModalBody>
          <ModalFooter display="flex" gap="$2">
            <Button
              onClick={() => {
                setMatchNames([])
                reset()
                closePreviewModal()
                onClose()
              }}
              colorScheme="neutral"
            >
              Cancel
            </Button>
            <Button
              onClick={() => {
                setMatchNames([])
                closePreviewModal()
                onOpen()
              }}
              colorScheme="neutral"
            >
              Back
            </Button>
            <Button
              loading={loading()}
              onClick={async () => {
                const resp = await ok(pathname(), matchNames())
                handleRespWithNotifySuccess(resp, () => {
                  setMatchNames([])
                  setSrcName("")
                  setNewName("")
                  reset()
                  refresh()
                  onClose()
                  closePreviewModal()
                })
              }}
              disabled={matchNames().length === 0}
            >
              OK
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </>
  )
}
