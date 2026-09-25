import {
  Box,
  ButtonGroup,
  css,
  Flex,
  Heading,
  IconButton,
  Input,
  Select,
  SelectContent,
  SelectIcon,
  SelectListbox,
  SelectOption,
  SelectOptionIndicator,
  SelectOptionText,
  SelectPlaceholder,
  SelectTrigger,
  SelectValue,
  Text,
  useColorModeValue,
  VStack,
} from "@hope-ui/solid"
import { For, Match, Switch } from "solid-js"
import { FaSolidMinus, FaSolidPlus } from "solid-icons/fa"
import { useT } from "~/hooks"
import { initialLocalSettings, local, LocalSetting, setLocal } from "~/store"

const hideSpinClass = css({
  "&::-webkit-inner-spin-button, &::-webkit-outer-spin-button": {
    "-webkit-appearance": "none",
    margin: 0,
  },
  "&[type=number]": {
    "-moz-appearance": "textfield",
  },
})

function LocalSettingControl(props: LocalSetting) {
  const t = useT()
  return (
    <Switch>
      <Match when={props.type === "select"}>
        <Select
          id={props.key}
          defaultValue={local[props.key]}
          onChange={(v) => setLocal(props.key, v)}
        >
          <SelectTrigger w={{ "@initial": "$full", "@sm": "240px" }}>
            <SelectPlaceholder>{t("global.choose")}</SelectPlaceholder>
            <SelectValue />
            <SelectIcon />
          </SelectTrigger>
          <SelectContent>
            <SelectListbox>
              <For each={props.options}>
                {(item) => (
                  <SelectOption value={item}>
                    <SelectOptionText>
                      {t(`home.local_settings.${props.key}_options.${item}`)}
                    </SelectOptionText>
                    <SelectOptionIndicator />
                  </SelectOption>
                )}
              </For>
            </SelectListbox>
          </SelectContent>
        </Select>
      </Match>
      <Match when={props.type === "number"}>
        <ButtonGroup attached w="fit-content">
          <IconButton
            aria-label="decrease"
            icon={<FaSolidMinus />}
            onClick={() => {
              setLocal(
                props.key,
                Math.max(1, parseInt(local[props.key]) - 1).toString(),
              )
            }}
          />
          <Input
            type="number"
            w="84px"
            textAlign="center"
            value={local[props.key]}
            onInput={(e) => {
              setLocal(props.key, e.currentTarget.value)
            }}
            borderRadius="$none"
            class={hideSpinClass()}
          />
          <IconButton
            aria-label="increase"
            icon={<FaSolidPlus />}
            onClick={() => {
              setLocal(props.key, (parseInt(local[props.key]) + 1).toString())
            }}
          />
        </ButtonGroup>
      </Match>
    </Switch>
  )
}

export const Appearance = () => {
  const t = useT()
  const cardBorder = useColorModeValue("$neutral4", "$neutral6")
  const cardBg = useColorModeValue("$background", "$neutral3")
  const dividerColor = useColorModeValue("$neutral4", "$neutral6")

  return (
    <Box
      w="$full"
      rounded="$xl"
      border="1px solid"
      borderColor={cardBorder()}
      bg={cardBg()}
      shadow="$sm"
      overflow="hidden"
    >
      <Box
        p={{ "@initial": "$4", "@sm": "$5" }}
        borderBottom="1px solid"
        borderColor={dividerColor()}
        bg={useColorModeValue("$neutral2", "$neutral4")()}
      >
        <Heading size={{ "@initial": "base", "@sm": "lg" }}>
          {t("manage.appearance")}
        </Heading>
        <Text fontSize="$sm" color="$neutral10" mt="$1">
          自定义文件界面的显示方式与交互行为
        </Text>
      </Box>
      <VStack spacing="$0" alignItems="stretch" w="$full">
        <For each={initialLocalSettings}>
          {(setting, idx) => (
            <Flex
              direction={{ "@initial": "column", "@sm": "row" }}
              justifyContent="space-between"
              alignItems={{ "@initial": "stretch", "@sm": "center" }}
              py="$4"
              px={{ "@initial": "$4", "@sm": "$5" }}
              borderBottom={
                idx() < initialLocalSettings.length - 1
                  ? "1px solid"
                  : "none"
              }
              borderColor={dividerColor()}
              gap="$3"
              _hover={{ bg: useColorModeValue("$neutral2", "$neutral4")() }}
              transition="background-color 0.15s"
            >
              <Text fontWeight="$medium" fontSize="$sm">
                {t(`home.local_settings.${setting.key}`)}
              </Text>
              <Box
                w={{ "@initial": "$full", "@sm": "auto" }}
                display="flex"
                justifyContent={{ "@initial": "flex-start", "@sm": "flex-end" }}
              >
                <LocalSettingControl {...setting} />
              </Box>
            </Flex>
          )}
        </For>
      </VStack>
    </Box>
  )
}
