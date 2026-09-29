import { Resp } from "~/types"
import { notify } from "./notify"

export const handleResp = <T>(
  resp: Resp<T>,
  success?: (data: T) => void,
  fail?: (message: string, code: number) => void,
  notify_error: boolean = true,
  notify_success?: boolean,
) => {
  if (resp.code === 200) {
    notify_success && notify.success(resp.message)
    success?.(resp.data)
  } else {
    notify_error && notify.error(resp.message)
    fail?.(resp.message, resp.code)
  }
}

export const handleRespWithoutNotify = <T>(
  resp: Resp<T>,
  success?: (data: T) => void,
  fail?: (message: string, code?: number) => void,
) => {
  return handleResp(resp, success, fail, false)
}

export const handleRespWithNotifySuccess = <T>(
  resp: Resp<T>,
  success?: (data: T) => void,
  fail?: (message: string, code?: number) => void,
  notify_error: boolean = true,
) => {
  return handleResp(resp, success, fail, notify_error, true)
}
