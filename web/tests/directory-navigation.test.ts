import assert from "node:assert/strict"
import test from "node:test"
import { fileURLToPath } from "node:url"
import { createServer } from "vite"

test("directory navigation tracks known directories and path joins correctly", async () => {
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
    const pathUtils = await server.ssrLoadModule("/src/utils/path.ts")

    // Verify initial state
    assert.equal(files.isKnownDirectoryPath("/snap"), false)
    assert.equal(files.isKnownDirectoryPath("/rulist"), false)

    // Simulate folder listing marking discovered directories
    files.rememberDirectoryPath("/snap", true)
    files.rememberDirectoryPath("/rulist", true)
    files.rememberDirectoryPath("/rulist/github", true)

    assert.equal(files.isKnownDirectoryPath("/snap"), true)
    assert.equal(files.isKnownDirectoryPath("/rulist"), true)
    assert.equal(files.isKnownDirectoryPath("/rulist/github"), true)
    assert.equal(files.isKnownDirectoryPath("/unknown"), false)

    // Verify path utilities prevent duplicate slashes
    assert.equal(pathUtils.pathJoin("/", "snap"), "/snap")
    assert.equal(pathUtils.pathJoin("/snap", "lxd"), "/snap/lxd")
    assert.equal(pathUtils.pathJoin("/rulist", "github"), "/rulist/github")

    // Verify reset clears known directory paths
    files.resetFileState()
    assert.equal(files.isKnownDirectoryPath("/snap"), false)
    assert.equal(files.isKnownDirectoryPath("/rulist"), false)
  } finally {
    await server?.close()
  }
})
