import { Link } from "@solidjs/router"
import { pathJoin, encodePath } from "~/utils"
import { useRouter } from "~/hooks"
import { ComponentProps } from "solid-js"

export const LinkWithBase = (
  props: ComponentProps<typeof Link> & { encode?: boolean },
) => (
  <Link
    {...props}
    href={pathJoin(props.encode ? encodePath(props.href) : props.href)}
  />
)

export const LinkWithPush = (props: ComponentProps<typeof LinkWithBase>) => {
  const { pushHref } = useRouter()
  return <LinkWithBase {...props} href={pushHref(props.href)} />
}
