import assert from "node:assert/strict"
import test from "node:test"
import { fileURLToPath } from "node:url"
import { createServer } from "vite"

import { getSafeRedirect, validateFilename } from "../src/utils/str.ts"

test("getSafeRedirect accepts only local absolute paths", () => {
  assert.equal(getSafeRedirect("/files/report.pdf"), "/files/report.pdf")
  assert.equal(getSafeRedirect("/"), "/")
  assert.equal(getSafeRedirect("https://example.com"), "")
  assert.equal(getSafeRedirect("//example.com/path"), "")
  assert.equal(getSafeRedirect("/\\example.com/path"), "")
  assert.equal(getSafeRedirect("files/report.pdf"), "")
  assert.equal(getSafeRedirect(null), "")
})

test("validateFilename rejects empty names and path separators", () => {
  assert.deepEqual(validateFilename("report.pdf"), { valid: true })
  assert.equal(validateFilename("   ").valid, false)
  assert.equal(validateFilename("../report.pdf").valid, false)
  assert.equal(validateFilename("folder/report.pdf").valid, false)
  assert.equal(validateFilename("folder\\report.pdf").valid, false)
  assert.equal(validateFilename("bad?.txt").valid, false)
})

test("shouldExpireSession expires protected 401 responses only", async () => {
  const documentDescriptor = Object.getOwnPropertyDescriptor(
    globalThis,
    "document",
  )
  const localStorageDescriptor = Object.getOwnPropertyDescriptor(
    globalThis,
    "localStorage",
  )
  const windowDescriptor = Object.getOwnPropertyDescriptor(globalThis, "window")
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: { addEventListener() {} },
  })
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: { getItem: () => null, setItem() {} },
  })
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { origin: "http://localhost" } },
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
    const { shouldExpireSession } = await server.ssrLoadModule(
      "/src/utils/request.ts",
    )

    assert.equal(shouldExpireSession(401, "/fs/list"), true)
    assert.equal(shouldExpireSession(401, "/fs/preview"), true)
    assert.equal(shouldExpireSession(401, "/fs/put"), true)
    assert.equal(shouldExpireSession(401, "/auth/login"), false)
    assert.equal(shouldExpireSession(400, "/auth/login"), false)
    assert.equal(shouldExpireSession(500, "/fs/list"), false)
  } finally {
    await server?.close()
    if (documentDescriptor) {
      Object.defineProperty(globalThis, "document", documentDescriptor)
    } else {
      Reflect.deleteProperty(globalThis, "document")
    }
    if (localStorageDescriptor) {
      Object.defineProperty(globalThis, "localStorage", localStorageDescriptor)
    } else {
      Reflect.deleteProperty(globalThis, "localStorage")
    }
    if (windowDescriptor) {
      Object.defineProperty(globalThis, "window", windowDescriptor)
    } else {
      Reflect.deleteProperty(globalThis, "window")
    }
  }
})
