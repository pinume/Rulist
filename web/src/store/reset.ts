import { clearAllHistory } from "./history"
import { resetFileState } from "./files"
import { setCurrentUser } from "./session"

export const resetSessionState = () => {
  clearAllHistory()
  resetFileState()
  setCurrentUser(null)
}
