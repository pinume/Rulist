import { Accessor, createSignal } from "solid-js"
import { EmptyResp, PResp } from "~/types"

export const useLoading = <T>(
  p: (...arg: any[]) => Promise<T>,
  fetch?: boolean,
  t?: boolean, // initial loading true
): [Accessor<typeof t>, typeof p] => {
  const [loading, setLoading] = createSignal(t)
  return [
    loading,
    async (...arg: any[]) => {
      setLoading(true)
      const data = await p(...arg)
      if (!fetch || (data as EmptyResp).code !== 401) {
        // why?
        // because if setLoading(false) here will rerender before navigate
        // maybe cause some bugs
        setLoading(false)
      }
      return data
    },
  ]
}

// Used together with handleResp
export const useFetch = <T>(
  p: (...arg: any[]) => Promise<T>,
  loading?: boolean,
): [Accessor<typeof loading>, typeof p] => {
  return useLoading(p, true, loading)
}
