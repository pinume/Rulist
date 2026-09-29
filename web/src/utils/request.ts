import axios from "axios"
import { resetSessionState } from "../store/reset"
import { bus } from "./bus"

export const api = window.location.origin

export const shouldExpireSession = (status?: number, url?: string) =>
  status === 401 && !url?.endsWith("/auth/login")

const instance = axios.create({
  baseURL: api + "/api",
  // timeout: 5000
  headers: {
    "Content-Type": "application/json;charset=utf-8",
    // 'Authorization': localStorage.getItem("admin-token") || "",
  },
  withCredentials: false,
})

// response interceptor
instance.interceptors.response.use(
  (response) => {
    return response.data
  },
  (error) => {
    // response error
    console.error(error) // for debug
    if (shouldExpireSession(error.response?.status, error.config?.url)) {
      changeToken()
      resetSessionState()

      if (!location.pathname.startsWith("/@login")) {
        bus.emit(
          "to",
          `/@login?redirect=${encodeURIComponent(location.pathname)}`,
        )
      }
    }
    if (error.response?.data) {
      return error.response.data
    }
    return {
      code: axios.isCancel(error) ? -1 : error.response?.status,
      message: error.message,
    }
  },
)

instance.defaults.headers.common["Authorization"] =
  localStorage.getItem("token") || ""

export const changeToken = (token?: string) => {
  instance.defaults.headers.common["Authorization"] = token ?? ""
  localStorage.setItem("token", token ?? "")
}

export { instance as r }
