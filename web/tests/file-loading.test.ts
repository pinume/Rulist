import assert from "node:assert/strict"
import test from "node:test"
import { fileURLToPath } from "node:url"
import { createServer } from "vite"

test("file loading waits for listings and ignores superseded work", async () => {
  const globals = ["document", "localStorage", "window"] as const
  const descriptors = globals.map((key) =>
    Object.getOwnPropertyDescriptor(globalThis, key),
  )
  let scrolls = 0
  Object.defineProperties(globalThis, {
    document: { configurable: true, value: { addEventListener() {} } },
    localStorage: {
      configurable: true,
      value: { getItem: () => null, setItem() {} },
    },
    window: {
      configurable: true,
      value: {
        location: { origin: "http://localhost" },
        scrollY: 12,
        scroll: () => {
          scrolls++
        },
      },
    },
  })
  let server: Awaited<ReturnType<typeof createServer>> | undefined
  try {
    const root = fileURLToPath(new URL("..", import.meta.url))
    server = await createServer({
      root,
      configFile: fileURLToPath(new URL("../vite.config.ts", import.meta.url)),
      server: { middlewareMode: true },
      appType: "custom",
      logLevel: "silent",
      plugins: [
        {
          name: "test-router",
          enforce: "pre",
          resolveId(id, importer) {
            if (
              id === "./useRouter" &&
              importer?.endsWith("/hooks/useFiles.ts")
            ) {
              return "\0test-router"
            }
            if (
              id === "./notify" &&
              importer?.endsWith("/utils/handle_resp.ts")
            ) {
              return "\0test-notify"
            }
          },
          load(id) {
            if (id === "\0test-router") {
              return 'export const useRouter = () => ({ pathname: () => "/unknown" })'
            }
            if (id === "\0test-notify") {
              return "export const notify = { success() {}, error() {} }"
            }
          },
          transform(_code, id) {
            // Keep the real API and response handling without importing browser UI.
            if (id.endsWith("/utils/index.ts")) {
              return 'export * from "./api"; export * from "./request"; export * from "./path"; export * from "./handle_resp"'
            }
          },
        },
      ],
    })
    const files = await server.ssrLoadModule("/src/store/files.ts")
    const history = await server.ssrLoadModule("/src/store/history.ts")
    const { r } = await server.ssrLoadModule("/src/utils/request.ts")
    const { useFiles } = await server.ssrLoadModule("/src/hooks/useFiles.ts")
    const requests: { url: string; data: any; respond: (data: any) => void }[] =
      []
    r.defaults.adapter = (config: any) =>
      new Promise((resolve) => {
        requests.push({
          url: config.url,
          data: JSON.parse(config.data),
          respond: (data) =>
            resolve({
              data: { code: 200, data },
              status: 200,
              statusText: "OK",
              headers: {},
              config,
            }),
        })
      })
    const tick = () => new Promise((resolve) => setTimeout(resolve))
    const hook = useFiles()

    // Refreshing an unknown directory must wait for both metadata and listing.
    let done = false
    const refreshing = hook.refresh().then(() => {
      done = true
    })
    await tick()
    assert.equal(requests[0].url, "/fs/get")
    requests[0].respond({ name: "unknown", is_dir: true })
    await tick()
    assert.equal(requests[1].url, "/fs/list")
    assert.equal(done, false)
    assert.equal(scrolls, 0)
    requests[1].respond({ content: [], total: 0 })
    await refreshing
    assert.equal(done, true)
    assert.equal(scrolls, 1)

    // A page removed by deletion must wait for the fallback page too.
    done = false
    const paging = hook.loadFolder("/unknown", 3).then(() => {
      done = true
    })
    await tick()
    requests[2].respond({ content: [], total: 1 })
    await tick()
    assert.equal(requests[3].data.page, 1)
    assert.equal(done, false)
    requests[3].respond({ content: [{ name: "remaining.txt" }], total: 1 })
    await paging
    assert.equal(files.fileStore.files[0].name, "remaining.txt")

    // Responses arriving out of order cannot overwrite the latest listing.
    const older = hook.loadFolder("/older")
    const newer = hook.loadFolder("/newer")
    await tick()
    requests[5].respond({ content: [{ name: "new.txt" }], total: 1 })
    await newer
    requests[4].respond({ content: [{ name: "old.txt" }], total: 1 })
    await older
    assert.equal(files.fileStore.files[0].name, "new.txt")

    // A new navigation must also cancel pending history restoration.
    history.recordHistory("/newer")
    const recovering = hook.loadPath("/newer")
    const navigating = hook.loadFolder("/destination")
    await tick()
    assert.equal(files.fileStore.state, files.ViewState.Loading)
    requests[6].respond({ content: [{ name: "destination.txt" }], total: 1 })
    await Promise.all([recovering, navigating])
    assert.equal(files.fileStore.files[0].name, "destination.txt")
    assert.equal(scrolls, 1)

    // Invalidation after restoring state must still prevent stale scrolling.
    history.recordHistory("/destination")
    const scrolling = hook.loadPath("/destination")
    await tick()
    files.resetFileState()
    await scrolling
    assert.equal(files.fileStore.state, files.ViewState.Initial)
    assert.equal(scrolls, 1)

    const { collectDownloadFiles } = await server.ssrLoadModule(
      "/src/utils/download.ts",
    )
    let active = 0
    let maximum = 0
    const visited: string[] = []
    const tree: Record<string, { name: string; is_dir: boolean }[]> = {
      "/archive/a": [
        { name: "nested", is_dir: true },
        { name: "a.txt", is_dir: false },
      ],
      "/archive/a/nested": [{ name: "deep.txt", is_dir: false }],
      "/archive/b": [{ name: "b.txt", is_dir: false }],
      "/archive/empty": [],
    }
    r.defaults.adapter = async (config: any) => {
      const { path } = JSON.parse(config.data)
      visited.push(path)
      maximum = Math.max(maximum, ++active)
      await tick()
      active--
      return {
        data: tree[path]
          ? { code: 200, data: { content: tree[path] } }
          : { code: 403, message: "Permission denied" },
        status: 200,
        statusText: "OK",
        headers: {},
        config,
      }
    }
    const downloadFiles = await collectDownloadFiles("/archive", [
      { name: "a", is_dir: true },
      { name: "b", is_dir: true },
      { name: "empty", is_dir: true },
      { name: "loose.txt", is_dir: false },
    ])
    assert.equal(maximum, 1)
    assert.deepEqual(
      downloadFiles.map((file: { path: string }) => file.path),
      ["a/nested/deep.txt", "a/a.txt", "b/b.txt", "loose.txt"],
    )
    await assert.rejects(
      collectDownloadFiles("/archive", [
        { name: "missing", is_dir: true },
        { name: "b", is_dir: true },
      ]),
      /Permission denied/,
    )
    assert.equal(visited.at(-1), "/archive/missing")
  } finally {
    await server?.close()
    globals.forEach((key, index) => {
      if (descriptors[index])
        Object.defineProperty(globalThis, key, descriptors[index]!)
      else Reflect.deleteProperty(globalThis, key)
    })
  }
})
