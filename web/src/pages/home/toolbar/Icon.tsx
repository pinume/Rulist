import { Icon, IconButton, Tooltip } from "@hope-ui/solid"
import { IconTypes } from "solid-icons"
import { useT } from "~/hooks"
import { getMainColor, me } from "~/store"
import { UserMethods, UserPermissionBits } from "~/types"
import { hoverColor } from "~/utils"
import { operations } from "./operations"

export const CenterIcon = (props: { name: string; onClick: () => void }) => {
  const index =
    UserPermissionBits[props.name as keyof typeof UserPermissionBits]
  if (index !== undefined && !UserMethods.can(me(), index)) return null
  const t = useT()
  const label = () => t(`home.toolbar.${props.name}`)
  return (
    <Tooltip placement="top" withArrow label={label()}>
      <IconButton
        class={`toolbar-${props.name}`}
        type="button"
        aria-label={label()}
        icon={<Icon as={operations[props.name]?.icon} boxSize="$full" />}
        onClick={props.onClick}
        compact
        boxSize="$7"
        p={operations[props.name]?.p ? "$1_5" : "$1"}
        bgColor="transparent"
        color={operations[props.name]?.color}
        _hover={{ bgColor: hoverColor() }}
      />
    </Tooltip>
  )
}

export const RightIcon = (props: {
  as: IconTypes
  tips?: string
  onClick: () => void
  p?: string
  class?: string
}) => {
  const t = useT()
  const label = () => t(`home.toolbar.${props.tips || "more"}`)
  return (
    <Tooltip placement="left" withArrow label={label()}>
      <IconButton
        class={props.class}
        type="button"
        aria-label={label()}
        icon={<Icon as={props.as} boxSize="$full" />}
        onClick={props.onClick}
        compact
        boxSize="$8"
        rounded="$lg"
        p={props.p || "$1"}
        bgColor="transparent"
        color={getMainColor()}
        _hover={{ bgColor: getMainColor(), color: "white" }}
      />
    </Tooltip>
  )
}
