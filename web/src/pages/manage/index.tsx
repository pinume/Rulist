import { Box, Button, Heading, HStack, useColorModeValue } from "@hope-ui/solid"
import { Navigate, Route, Routes } from "@solidjs/router"
import { JSXElement, Show } from "solid-js"
import { FiHome } from "solid-icons/fi"
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
    { path: "/@settings", title: "manage.appearance" },
    { path: "/@settings/profile", title: "manage.sidemenu.profile" },
    { path: "/@settings/users", title: "manage.sidemenu.users", admin: true },
  ]

  return (
    <Box
      minH="100vh"
      w="$full"
      bgColor={useColorModeValue("$background", "$neutral2")()}
      p={{ "@initial": "$3", "@sm": "$4" }}
    >
      <Box w="$full" maxW="1200px" mx="auto">
        <HStack
          justifyContent="space-between"
          alignItems="center"
          mb={{ "@initial": "$3", "@sm": "$4" }}
        >
          <Heading size={{ "@initial": "lg", "@sm": "xl" }}>
            {t("manage.title")}
          </Heading>
          <Button
            leftIcon={<FiHome />}
            variant="subtle"
            size={{ "@initial": "sm", "@sm": "md" }}
            onClick={() => to("/")}
          >
            {t("manage.sidemenu.home")}
          </Button>
        </HStack>
        <HStack
          spacing="$2"
          wrap="wrap"
          mb={{ "@initial": "$4", "@sm": "$6" }}
        >
          {tabs
            .filter((tab) => !tab.admin || UserMethods.is_admin(me()))
            .map((tab) => (
              <Button
                size={{ "@initial": "sm", "@sm": "md" }}
                variant={
                  pathname() === tab.path ||
                  (tab.path !== "/@settings" &&
                    pathname().startsWith(tab.path + "/")) ||
                  (tab.path.endsWith("/profile") &&
                    pathname() === "/@settings/2fa")
                    ? "solid"
                    : "ghost"
                }
                onClick={() => to(tab.path)}
              >
                {t(tab.title)}
              </Button>
            ))}
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
