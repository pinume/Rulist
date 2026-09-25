import { Box, Button, Heading, HStack, useColorModeValue } from "@hope-ui/solid"
import { Navigate, Route, Routes } from "@solidjs/router"
import { JSXElement, Show } from "solid-js"
import { FiArrowLeft, FiSliders, FiUser, FiUsers } from "solid-icons/fi"
import { useRouter, useT, useTitle } from "~/hooks"
import { me } from "~/store"
import { UserMethods } from "~/types"
import { joinBase } from "~/utils"
import { Appearance } from "./Appearance"
import Profile from "./users/Profile"
import Users from "./users/Users"
import UserEdit from "./users/AddOrEdit"
import TwoFA from "./users/2fa"

const AdminOnly = (props: { children: JSXElement }) => (
  <Show
    when={UserMethods.is_admin(me())}
    fallback={<Navigate href={joinBase("/@settings")} />}
  >
    {props.children}
  </Show>
)

const Settings = () => {
  const t = useT()
  const { pathname, to } = useRouter()
  useTitle(() => t("manage.title"))
  const tabs = [
    { path: "/@settings", title: "manage.appearance", icon: FiSliders },
    { path: "/@settings/profile", title: "manage.sidemenu.profile", icon: FiUser },
    { path: "/@settings/users", title: "manage.sidemenu.users", admin: true, icon: FiUsers },
  ]

  return (
    <Box
      minH="100vh"
      w="$full"
      bgColor={useColorModeValue("$background", "$neutral2")()}
      p={{ "@initial": "$3", "@sm": "$6" }}
    >
      <Box w="$full" maxW="1000px" mx="auto">
        <HStack
          justifyContent="space-between"
          alignItems="center"
          mb="$6"
          pb="$4"
          borderBottom="1px solid"
          borderColor={useColorModeValue("$neutral4", "$neutral6")()}
        >
          <HStack spacing="$3">
            <Button
              leftIcon={<FiArrowLeft />}
              variant="outline"
              size="sm"
              onClick={() => to("/")}
            >
              {t("manage.sidemenu.home")}
            </Button>
            <Heading size="lg">
              {t("manage.title")}
            </Heading>
          </HStack>
        </HStack>

        <HStack
          spacing="$1"
          p="$1"
          bg={useColorModeValue("$neutral3", "$neutral5")()}
          rounded="$xl"
          w="fit-content"
          maxW="$full"
          overflowX="auto"
          mb="$6"
        >
          {tabs
            .filter((tab) => !tab.admin || UserMethods.is_admin(me()))
            .map((tab) => {
              const isActive = () =>
                pathname() === tab.path ||
                (tab.path !== "/@settings" && pathname().startsWith(tab.path + "/")) ||
                (tab.path.endsWith("/profile") && pathname() === "/@settings/2fa")

              return (
                <Button
                  size="sm"
                  leftIcon={<tab.icon />}
                  variant={isActive() ? "solid" : "ghost"}
                  colorScheme={isActive() ? "accent" : "neutral"}
                  bg={isActive() ? useColorModeValue("white", "$neutral7")() : "transparent"}
                  color={isActive() ? useColorModeValue("$neutral12", "white")() : useColorModeValue("$neutral10", "$neutral9")()}
                  shadow={isActive() ? "$xs" : "none"}
                  rounded="$lg"
                  fontWeight={isActive() ? "$semibold" : "$medium"}
                  _hover={isActive() ? undefined : {
                    bg: useColorModeValue("$neutral4", "$neutral6")(),
                    color: useColorModeValue("$neutral12", "white")(),
                  }}
                  onClick={() => to(tab.path)}
                >
                  {t(tab.title)}
                </Button>
              )
            })}
        </HStack>
        <Routes>
          <Route path="" component={Appearance} />
          <Route path="/profile" component={Profile} />
          <Route path="/2fa" component={TwoFA} />
          <Route
            path="/users"
            element={
              <AdminOnly>
                <Users />
              </AdminOnly>
            }
          />
          <Route
            path="/users/add"
            element={
              <AdminOnly>
                <UserEdit />
              </AdminOnly>
            }
          />
          <Route
            path="/users/edit/:id"
            element={
              <AdminOnly>
                <UserEdit />
              </AdminOnly>
            }
          />
        </Routes>
      </Box>
    </Box>
  )
}

export default Settings
