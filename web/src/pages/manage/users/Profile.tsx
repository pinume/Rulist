import {
  Badge,
  Box,
  Button,
  createDisclosure,
  Flex,
  FormControl,
  FormHelperText,
  FormLabel,
  Heading,
  HStack,
  Input,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
  ModalOverlay,
  SimpleGrid,
  Text,
  useColorModeValue,
  VStack,
} from "@hope-ui/solid"
import { createSignal, For, Show } from "solid-js"
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
  const [passwordTouched, setPasswordTouched] = createSignal(false)
  const [otpCode, setOtpCode] = createSignal("")
  const [loading, save] = useFetch((): PEmptyResp =>
    r.post("/me/update", {
      username: username(),
      password:
        passwordTouched() && (!UserMethods.is_admin(me()) || password())
          ? password()
          : undefined,
      current_password:
        (passwordTouched() && (!UserMethods.is_admin(me()) || password())) ||
        username() !== me().username
          ? currentPassword()
          : undefined,
    }),
  )
  const [logoutLoading, logout] = useFetch((): PEmptyResp =>
    r.get("/auth/logout"),
  )
  const [disable2faLoading, disable2fa] = useFetch((): PEmptyResp =>
    r.post("/auth/2fa/disable", { code: otpCode() }),
  )
  const {
    isOpen: isDisable2faOpen,
    onOpen: onOpenDisable2fa,
    onClose: onCloseDisable2fa,
  } = createDisclosure()

  const handleDisable2fa = async () => {
    if (!otpCode().trim()) {
      notify.warning(t("users.input_code"))
      return
    }
    handleResp(await disable2fa(), () => {
      setMe({ ...me(), otp: false })
      setOtpCode("")
      onCloseDisable2fa()
      notify.success(t("users.cancel_2fa_success"))
    })
  }

  const saveMe = async () => {
    if (passwordTouched() && password() !== confirmPassword()) {
      notify.warning(t("users.confirm_password_not_same"))
      return
    }
    if (
      me().password_unset &&
      (UserMethods.is_admin(me()) ? !passwordTouched() || !password() : !passwordTouched())
    ) {
      notify.warning(t("users.password_required"))
      return
    }
    if (
      UserMethods.is_admin(me()) &&
      !me().password_unset &&
      ((passwordTouched() && password()) || username() !== me().username)
    ) {
      if (!currentPassword()) {
        notify.warning(t("users.current_password_empty"))
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
                disabled={UserMethods.is_admin(me())}
                onInput={(e) => setUsername(e.currentTarget.value)}
              />
            </FormControl>

            <Show when={!me().password_unset}>
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
            </Show>

            <SimpleGrid gap="$3" columns={{ "@initial": 1, "@sm": 2 }} w="$full">
              <FormControl w="$full">
                <FormLabel for="password">{t("users.change_password")}</FormLabel>
                <Input
                  id="password"
                  w="$full"
                  type="password"
                  placeholder="********"
                  value={password()}
                  onInput={(e) => {
                    setPasswordTouched(true)
                    setPassword(e.currentTarget.value)
                  }}
                />
                <FormHelperText>{t("users.change_password-tips")}</FormHelperText>
                <Show when={!UserMethods.is_admin(me())}>
                  <Button
                    mt="$2"
                    size="sm"
                    variant="subtle"
                    onClick={() => {
                      setPasswordTouched(true)
                      setPassword("")
                      setConfirmPassword("")
                    }}
                  >
                    {t("users.set_empty_password")}
                  </Button>
                </Show>
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
              <Show when={!me().otp && !me().password_unset}>
                <Button
                  variant="subtle"
                  onClick={() => {
                    to("/@settings/2fa")
                  }}
                >
                  {t("users.enable_2fa")}
                </Button>
              </Show>
              <Show when={me().otp}>
                <Button
                  variant="subtle"
                  colorScheme="danger"
                  onClick={onOpenDisable2fa}
                >
                  {t("users.cancel_2fa")}
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

      <Modal
        blockScrollOnMount={false}
        opened={isDisable2faOpen()}
        onClose={() => {
          onCloseDisable2fa()
          setOtpCode("")
        }}
        initialFocus="#disable-2fa-code"
      >
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>{t("users.cancel_2fa")}</ModalHeader>
          <ModalBody>
            <FormControl w="$full">
              <FormLabel for="disable-2fa-code">{t("users.input_code")}</FormLabel>
              <Input
                id="disable-2fa-code"
                inputMode="numeric"
                placeholder={t("users.input_code")}
                value={otpCode()}
                onInput={(e) => setOtpCode(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    handleDisable2fa()
                  }
                }}
              />
            </FormControl>
          </ModalBody>
          <ModalFooter display="flex" gap="$2">
            <Button
              onClick={() => {
                onCloseDisable2fa()
                setOtpCode("")
              }}
              colorScheme="neutral"
            >
              {t("global.cancel")}
            </Button>
            <Button
              colorScheme="danger"
              loading={disable2faLoading()}
              onClick={handleDisable2fa}
            >
              {t("global.confirm")}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </VStack>
  )
}

export default Profile
