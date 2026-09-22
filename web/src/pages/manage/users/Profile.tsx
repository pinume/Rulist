import {
  Badge,
  Box,
  Button,
  Flex,
  FormControl,
  FormHelperText,
  FormLabel,
  Heading,
  HStack,
  Input,
  SimpleGrid,
  Text,
  useColorModeValue,
  VStack,
} from "@hope-ui/solid"
import { createSignal, For, JSXElement, onCleanup, Show } from "solid-js"
import { useFetch, useManageTitle, useRouter, useT } from "~/hooks"
import { setMe, me, getSettingBool } from "~/store"
import {
  PEmptyResp,
  UserMethods,
  UserPermissionBits,
  UserPermissions,
} from "~/types"
import { changeToken, handleResp, notify, r } from "~/utils"

const Profile = () => {
  const t = useT()
  useManageTitle("manage.sidemenu.profile")
  const { to } = useRouter()
  const [username, setUsername] = createSignal(me().username)
  const [currentPassword, setCurrentPassword] = createSignal("")
  const [password, setPassword] = createSignal("")
  const [confirmPassword, setConfirmPassword] = createSignal("")
  const [loading, save] = useFetch((): PEmptyResp =>
    r.post("/me/update", {
      username: username(),
      password: password(),
      current_password: currentPassword(),
    }),
  )
  const [logoutLoading, logout] = useFetch((): PEmptyResp =>
    r.get("/auth/logout"),
  )

  const saveMe = async () => {
    if (password() || username() !== me().username) {
      if (!currentPassword()) {
        notify.warning(t("users.current_password_empty"))
        return
      }
      if (password() && password() !== confirmPassword()) {
        notify.warning(t("users.confirm_password_not_same"))
        return
      }
    }
    const resp = await save()
    handleResp(resp, () => {
      setMe({ ...me(), username: username() })
      notify.success(t("users.update_profile_success"))
      to(`/@login?redirect=${encodeURIComponent(location.pathname)}`)
    })
  }

  const cardBorder = useColorModeValue("$neutral4", "$neutral6")
  const cardBg = useColorModeValue("$background", "$neutral3")
  const dividerColor = useColorModeValue("$neutral4", "$neutral6")

  return (
    <VStack w="$full" spacing="$4" alignItems="stretch">
      {/* 个人资料与修改密码卡片 */}
      <Box
        w="$full"
        rounded="$lg"
        border="1px solid"
        borderColor={cardBorder()}
        bg={cardBg()}
        shadow="$xs"
        overflow="hidden"
      >
        {/* 卡片头部：用户信息摘要 */}
        <Box
          p={{ "@initial": "$4", "@sm": "$5" }}
          borderBottom="1px solid"
          borderColor={dividerColor()}
        >
          <HStack justifyContent="space-between" alignItems="center">
            <HStack spacing="$3" alignItems="center">
              <Box
                boxSize="$10"
                rounded="$full"
                bg="$primary9"
                color="white"
                display="flex"
                alignItems="center"
                justifyContent="center"
                fontWeight="$bold"
                fontSize="$lg"
                userSelect="none"
              >
                {(username() || "U").slice(0, 1).toUpperCase()}
              </Box>
              <VStack alignItems="start" spacing="$0">
                <Text fontWeight="$bold" fontSize="$md">
                  {username()}
                </Text>
                <Text fontSize="$xs" color="$neutral10">
                  {UserMethods.is_admin(me()) ? "系统管理员" : "普通用户"}
                </Text>
              </VStack>
            </HStack>
            <Badge
              colorScheme={UserMethods.is_admin(me()) ? "accent" : "info"}
              variant="subtle"
            >
              {UserMethods.is_admin(me()) ? "管理员" : "普通用户"}
            </Badge>
          </HStack>
        </Box>

        {/* 表单主体 */}
        <Box p={{ "@initial": "$4", "@sm": "$5" }}>
          <VStack spacing="$4" alignItems="stretch" maxW="640px">
            <FormControl w="$full">
              <FormLabel for="username">{t("users.change_username")}</FormLabel>
              <Input
                id="username"
                w="$full"
                value={username()}
                onInput={(e) => setUsername(e.currentTarget.value)}
              />
            </FormControl>

            <FormControl w="$full">
              <FormLabel for="current-password">{t("users.current_password")}</FormLabel>
              <Input
                id="current-password"
                w="$full"
                type="password"
                placeholder="********"
                value={currentPassword()}
                onInput={(e) => setCurrentPassword(e.currentTarget.value)}
              />
              <FormHelperText>{t("users.current_password-tips")}</FormHelperText>
            </FormControl>

            <SimpleGrid gap="$3" columns={{ "@initial": 1, "@sm": 2 }} w="$full">
              <FormControl w="$full">
                <FormLabel for="password">{t("users.change_password")}</FormLabel>
                <Input
                  id="password"
                  w="$full"
                  type="password"
                  placeholder="********"
                  value={password()}
                  onInput={(e) => setPassword(e.currentTarget.value)}
                />
                <FormHelperText>{t("users.change_password-tips")}</FormHelperText>
              </FormControl>
              <FormControl w="$full">
                <FormLabel for="confirm-password">
                  {t("users.confirm_password")}
                </FormLabel>
                <Input
                  id="confirm-password"
                  w="$full"
                  type="password"
                  placeholder="********"
                  value={confirmPassword()}
                  onInput={(e) => setConfirmPassword(e.currentTarget.value)}
                />
                <FormHelperText>{t("users.confirm_password-tips")}</FormHelperText>
              </FormControl>
            </SimpleGrid>

            {/* 操作按钮组 */}
            <HStack spacing="$3" pt="$3" wrap="wrap">
              <Button
                colorScheme="accent"
                loading={loading()}
                onClick={[saveMe, false]}
              >
                {t("global.save")}
              </Button>
              <Show when={!me().otp}>
                <Button
                  variant="subtle"
                  onClick={() => {
                    to("/@settings/2fa")
                  }}
                >
                  {t("users.enable_2fa")}
                </Button>
              </Show>
              <Button
                variant="ghost"
                colorScheme="danger"
                loading={logoutLoading()}
                onClick={async () => {
                  handleResp(await logout(), () => {
                    changeToken()
                    notify.success(t("manage.logout_success"))
                    to("/@login?redirect=%2F")
                  })
                }}
              >
                {t("manage.logout")}
              </Button>
            </HStack>
          </VStack>
        </Box>
      </Box>

      {/* 权限卡片 */}
      <Box
        w="$full"
        rounded="$lg"
        border="1px solid"
        borderColor={cardBorder()}
        bg={cardBg()}
        shadow="$xs"
        p={{ "@initial": "$4", "@sm": "$5" }}
      >
        <VStack w="$full" alignItems="stretch" spacing="$3">
          <Box>
            <Heading size="base">{t("users.permission")}</Heading>
            <Text fontSize="$xs" color="$neutral10" mt="$1">
              当前账户所拥有的系统权限清单
            </Text>
          </Box>
          <Flex wrap="wrap" gap="$2">
            <For each={UserPermissions}>
              {(item) => {
                const can = UserMethods.can(me(), UserPermissionBits[item])
                return (
                  <Badge
                    colorScheme={can ? "success" : "neutral"}
                    variant={can ? "subtle" : "outline"}
                    px="$2_5"
                    py="$1"
                    fontSize="$xs"
                  >
                    {t(`users.permissions.${item}`)}
                  </Badge>
                )
              }}
            </For>
          </Flex>
        </VStack>
      </Box>
    </VStack>
  )
}

export default Profile
