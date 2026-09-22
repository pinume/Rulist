const hash_salt = "https://github.com/alist-org/alist"

export async function hashPwd(pwd: string): Promise<string> {
  const msgBuffer = new TextEncoder().encode(`${pwd}-${hash_salt}`)
  const hashBuffer = await crypto.subtle.digest("SHA-256", msgBuffer)
  const hashArray = Array.from(new Uint8Array(hashBuffer))
  return hashArray.map((b) => b.toString(16).padStart(2, "0")).join("")
}
