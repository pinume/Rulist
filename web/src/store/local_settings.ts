import { createLocalStorage } from "@solid-primitives/storage"

const [local, setLocal] = createLocalStorage()

if (!local["folder_sort_position"]) {
  setLocal("folder_sort_position", "top")
}

export { local, setLocal }
