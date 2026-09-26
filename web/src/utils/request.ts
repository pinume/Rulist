import axios from "axios"
import { api } from "."

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
