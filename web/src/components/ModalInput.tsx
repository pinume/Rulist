import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  Button,
  Input,
  Textarea,
  FormHelperText,
} from "@hope-ui/solid"
import { createSignal, JSXElement, Show, createEffect, onCleanup } from "solid-js"
import { notify, validateFilename } from "~/utils"

export type ModalInputProps = {
  opened: boolean
  onClose: () => void
  title: string
  isRenamingFile?: boolean
  onSubmit?: (text: string) => void
  type?: string
  defaultValue?: string
  loading?: boolean
  tips?: string
  topSlot?: JSXElement
  bottomSlot?: JSXElement
  footerSlot?: JSXElement
  onDrop?: (e: DragEvent, setValue: (value: string) => void) => void
  validateFilename?: boolean
}

const validationMessage = (error?: string) =>
  error === "invalid_filename_chars"
    ? 'File names cannot contain: / \\ ? < > * : | "'
    : "Please enter a value"

export const ModalInput = (props: ModalInputProps) => {
  const [value, setValue] = createSignal(props.defaultValue ?? "")
  const [validationError, setValidationError] = createSignal<string>("")
  let inputRef: HTMLInputElement | HTMLTextAreaElement

  const handleFocus = () => {
    const dotIndex = value().lastIndexOf(".")
    setTimeout(() => {
      if (dotIndex > 0 && props.isRenamingFile) {
        inputRef.setSelectionRange(0, dotIndex)
      } else {
        inputRef.select()
      }
    }, 10)
  }

  createEffect(() => {
    if (inputRef) {
      inputRef.focus()
      handleFocus()
    }
    onCleanup(() => {
      if (inputRef) inputRef.setSelectionRange(0, 0)
    })
  })

  createEffect(() => {
    if (!props.opened) setValue("")
  })

  const submit = () => {
    if (props.validateFilename) {
      const validation = validateFilename(value())
      if (!validation.valid) {
        notify.warning(validationMessage(validation.error))
        return
      }
    } else if (!value() || value().trim().length === 0) {
      notify.warning("Please enter a value")
      return
    }
    props.onSubmit?.(value())
  }

  const handleInput = (newValue: string) => {
    setValue(newValue)
    if (props.validateFilename) {
      const validation = validateFilename(newValue)
      setValidationError(validation.valid ? "" : validation.error || "")
    } else {
      setValidationError("")
    }
  }

  return (
    <Modal
      blockScrollOnMount={false}
      opened={props.opened}
      onClose={props.onClose}
      initialFocus="#modal-input"
    >
      <ModalOverlay />
      <ModalContent onDrop={(e: DragEvent) => props.onDrop?.(e, setValue)}>
        <ModalHeader>{props.title}</ModalHeader>
        <ModalBody>
          <Show when={props.topSlot}>{props.topSlot}</Show>
          <Show
            when={props.type === "text"}
            fallback={
              <Input
                id="modal-input"
                type={props.type}
                value={value()}
                ref={(el) => (inputRef = el)}
                invalid={!!validationError()}
                onInput={(e) => handleInput(e.currentTarget.value)}
                onFocus={handleFocus}
                onKeyDown={(e) => {
                  if (e.key === "Enter") submit()
                }}
              />
            }
          >
            <Textarea
              id="modal-input"
              value={value()}
              ref={(el) => (inputRef = el)}
              invalid={!!validationError()}
              onInput={(e) => handleInput(e.currentTarget.value)}
              onFocus={handleFocus}
            />
          </Show>
          <Show when={validationError()}>
            <FormHelperText color="$danger9">
              {validationMessage(validationError())}
            </FormHelperText>
          </Show>
          <Show when={props.tips}>
            <FormHelperText>{props.tips}</FormHelperText>
          </Show>
          <Show when={props.bottomSlot}>{props.bottomSlot}</Show>
        </ModalBody>
        <ModalFooter display="flex" gap="$2">
          <Show when={props.footerSlot}>{props.footerSlot}</Show>
          <Button onClick={props.onClose} colorScheme="neutral">Cancel</Button>
          <Button loading={props.loading} onClick={submit}>OK</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
