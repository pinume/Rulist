import { createLocalStorage } from "@solid-primitives/storage"

const [local, setLocal] = createLocalStorage()

export const initialLocalSettings: any[] = []
if (!local["folder_sort_position"]) {
  setLocal("folder_sort_position", "top")
}

export { local, setLocal }
