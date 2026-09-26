import {
  Image,
  Center,
  Flex,
  Heading,
  Input,
  Button,
  useColorModeValue,
  HStack,
  VStack,
  Checkbox,
  FormControl,
  FormLabel,
} from "@hope-ui/solid"
import { createMemo, createSignal, Show } from "solid-js"
import { useFetch, useLoading, useT, useTitle, useRouter } from "~/hooks"
import {
  changeToken,
  r,
  notify,
  handleRespWithoutAuthAndNotify,
  base_path,
} from "~/utils"
import { Resp } from "~/types"
import LoginBg from "./LoginBg"
import { createStorageSignal } from "@solid-primitives/storage"
import { getLogo, getSetting, getSettingBool } from "~/store"
import { joinBase } from "~/utils"

const Login = () => {
  const [lightLogo, darkLogo] = getLogo()
  const logo = useColorModeValue(lightLogo, darkLogo)
  const logoSrc = createMemo(() => {
    const value = logo()
    if (/^(?:https?:)?\/\//.test(value) || /^(?:data|blob):/.test(value)) {
      return value
    }
    return joinBase(value)
  })
  const t = useT()
  const title = createMemo(() => {
    return `${t("login.login_to")} ${getSetting("site_title")}`
  })
  useTitle(title)
  const bgColor = useColorModeValue("white", "$neutral3")
  const [username, setUsername] = createSignal(
    localStorage.getItem("username") || "",
  )
  const [password, setPassword] = createSignal("")
  const [opt, setOpt] = createSignal("")
  const [remember, setRemember] = createStorageSignal("remember-pwd", "false")
  const [loading, data] = useLoading(
    async (): Promise<Resp<{ token: string }>> => {
      return r.post("/auth/login", {
        username: username(),
        password: password(),
        otp_code: opt(),
      })
    },
  )
  const { to } = useRouter()
  const Login = async () => {
    if (remember() === "true") {
      localStorage.setItem("username", username())
    } else {
      localStorage.removeItem("username")
    }
    const resp = await data()
    handleRespWithoutAuthAndNotify(
      resp,
      (data) => {
        notify.success(t("login.success"))
        changeToken(data.token)
        to(base_path || "/", true)
      },
      (msg, code) => {
        if (!needOpt() && code === 402) {
          setNeedOpt(true)
        } else {
          notify.error(msg)
        }
      },
    )
  }
  const [needOpt, setNeedOpt] = createSignal(false)
  return (
    <Center zIndex="$docked" w="$full" h="100vh">
      <VStack
        bgColor={bgColor()}
        rounded="$xl"
        p={{ "@initial": "$5", "@sm": "$6" }}
        w={{
          "@initial": "90%",
          "@sm": "364px",
        }}
        spacing="$4"
        border="1px solid"
        borderColor={useColorModeValue(
          "rgba(148, 163, 184, 0.28)",
          "$neutral6",
        )()}
        shadow="xl"
      >
        <Flex alignItems="center" justifyContent="space-around">
          <Image mr="$2" boxSize="$12" src={logoSrc()} />
          <Heading color="$info9" fontSize="$2xl">
            {title()}
          </Heading>
        </Flex>
        <Show
          when={!needOpt()}
          fallback={
            <FormControl>
              <FormLabel for="totp">{t("login.otp")}</FormLabel>
              <Input
                id="totp"
                name="otp"
                placeholder={t("login.otp-tips")}
                value={opt()}
                onInput={(e) => setOpt(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    Login()
                  }
                }}
              />
            </FormControl>
          }
        >
          <FormControl>
            <FormLabel for="username">{t("login.username")}</FormLabel>
            <Input
              id="username"
              name="username"
              placeholder={t("login.username-tips")}
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
            />
          </FormControl>
          <FormControl>
            <FormLabel for="password">{t("login.password")}</FormLabel>
            <Input
              id="password"
              name="password"
              placeholder={t("login.password-tips")}
              type="password"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  Login()
                }
              }}
            />
          </FormControl>
          <Flex
            px="$1"
            w="$full"
            fontSize="$sm"
            color="$neutral10"
            justifyContent="space-between"
            alignItems="center"
          >
            <Checkbox
              checked={remember() === "true"}
              onChange={() =>
                setRemember(remember() === "true" ? "false" : "true")
              }
            >
              {t("login.remember")}
            </Checkbox>
          </Flex>
        </Show>
        <HStack w="$full" spacing="$2">
          <Button
            colorScheme="primary"
            w="$full"
            onClick={() => {
              if (needOpt()) {
                setOpt("")
              } else {
                setUsername("")
                setPassword("")
              }
            }}
          >
            {t("login.clear")}
          </Button>
          <Button w="$full" loading={loading()} onClick={Login}>
            {t("login.login")}
          </Button>
        </HStack>
      </VStack>
      <LoginBg />
    </Center>
  )
}

export default Login
