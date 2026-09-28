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
import { useLoading, useTitle, useRouter } from "~/hooks"
import {
  changeToken,
  r,
  notify,
  handleRespWithoutAuthAndNotify,
  getSafeRedirect,
} from "~/utils"
import { Resp } from "~/types"
import LoginBg from "./LoginBg"
import { createStorageSignal } from "@solid-primitives/storage"
import { getLogo, getSetting } from "~/store"
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
  const title = createMemo(() => `Sign in to ${getSetting("site_title")}`)
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
  const { to, searchParams } = useRouter()
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
        notify.success("Signed in successfully")
        changeToken(data.token)
        const redirect = getSafeRedirect(searchParams["redirect"])
        to(redirect || "/", true)
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
              <FormLabel for="totp">Verification code</FormLabel>
              <Input
                id="totp"
                name="otp"
                placeholder="Enter your OTP code"
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
            <FormLabel for="username">Username</FormLabel>
            <Input
              id="username"
              name="username"
              placeholder="Enter your username"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
            />
          </FormControl>
          <FormControl>
            <FormLabel for="password">Password</FormLabel>
            <Input
              id="password"
              name="password"
              placeholder="Enter your password"
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
              Remember username
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
            Clear
          </Button>
          <Button w="$full" loading={loading()} onClick={Login}>
            Sign in
          </Button>
        </HStack>
      </VStack>
      <LoginBg />
    </Center>
  )
}

export default Login
