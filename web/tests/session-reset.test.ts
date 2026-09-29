import assert from "node:assert/strict"
import test from "node:test"
import { fileURLToPath } from "node:url"
import { createServer } from "vite"

test("resetSessionState clears all user-scoped browser state", async () => {
  const documentDescriptor = Object.getOwnPropertyDescriptor(globalThis, "document")
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: { addEventListener() {} },
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
    })
    const files = await server.ssrLoadModule("/src/store/files.ts")
    const history = await server.ssrLoadModule("/src/store/history.ts")
    const session = await server.ssrLoadModule("/src/store/session.ts")
    const { resetSessionState } = await server.ssrLoadModule(
      "/src/store/reset.ts",
    )
    const generationBefore = files.getFileRequestGeneration()

    session.setCurrentUser({
      id: 1,
      username: "alice",
      role: 0,
      permission: 0,
      otp: false,
    })
    files.FileStore.setFile({
      name: "old.txt",
      size: 3,
      is_dir: false,
      modified: "2026-01-01T00:00:00Z",
      type: 0,
    })
    files.FileStore.setRawUrl("/old.txt?sign=stale")
    files.FileStore.setListing(
      [
        {
          name: "old.txt",
          size: 3,
          is_dir: false,
          modified: "",
          type: 0,
          selected: true,
        },
      ],
      1,
      1,
    )
    files.FileStore.setState(files.ViewState.File)
    files.setDirectoryFilter("old")
    files.setLastClickedIndex(0)
    files.rememberDirectoryPath("/old", true)
    files.setUploadConfig({ asTask: true, overwrite: true })
    files.setShouldKeepState(true)
    history.HistoryMap.set("/old", { state: {}, scroll: 12 })

    resetSessionState()

    assert.equal(session.currentUser(), null)
    assert.equal(files.fileStore.state, files.ViewState.Initial)
    assert.deepEqual(files.fileStore.file, {})
    assert.equal(files.fileStore.raw_url, "")
    assert.deepEqual(files.fileStore.files, [])
    assert.deepEqual(files.selectedFiles(), [])
    assert.equal(files.fileStore.total, 0)
    assert.equal(files.fileStore.page, 1)
    assert.equal(files.directoryFilter(), "")
    assert.equal(files.isKnownDirectoryPath("/old"), false)
    assert.deepEqual(files.uploadConfig, { asTask: false, overwrite: false })
    assert.equal(files.shouldKeepState(), false)
    assert.equal(history.HistoryMap.size, 0)
    assert.equal(files.getFileRequestGeneration(), generationBefore + 1)
  } finally {
    await server?.close()
    if (documentDescriptor) {
      Object.defineProperty(globalThis, "document", documentDescriptor)
    } else {
      Reflect.deleteProperty(globalThis, "document")
    }
  }
})
