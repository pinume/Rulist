import assert from "node:assert/strict"
import test from "node:test"

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
