import {
  Box,
  Button,
  Checkbox,
  FormControl,
  FormLabel,
  Heading,
  HStack,
  Input,
  SimpleGrid,
  Skeleton,
  Text,
  useColorModeValue,
  VStack,
} from "@hope-ui/solid"
import { useFetch, useRouter, useT } from "~/hooks"
import { handleResp, notify, r } from "~/utils"
import {
  PEmptyResp,
  PResp,
  User,
  UserMethods,
  UserPermissionBits,
  UserPermissions,
  UserRole,
} from "~/types"
import { createStore } from "solid-js/store"
import { For, Show } from "solid-js"
import { Me, me, setMe } from "~/store"

const Permission = (props: {
  can: boolean
  onChange: (val: boolean) => void
  name: string
}) => {
  const t = useT()
  const activeBg = useColorModeValue("$primary2", "$primary4")
  const inactiveBg = useColorModeValue("$background", "$neutral2")
  const activeBorder = useColorModeValue("$primary7", "$primary8")
  const inactiveBorder = useColorModeValue("$neutral4", "$neutral6")

  return (
    <Box
      display="flex"
      alignItems="center"
      justifyContent="space-between"
      gap="$2"
      rounded="$md"
      border="1px solid"
      borderColor={props.can ? activeBorder() : inactiveBorder()}
      bg={props.can ? activeBg() : inactiveBg()}
      px="$3"
      py="$2_5"
      cursor="pointer"
      onClick={() => props.onChange(!props.can)}
    >
      <Text fontSize="$sm" userSelect="none">
        {t(`users.permissions.${props.name}`)}
      </Text>
      <Checkbox checked={props.can} pointerEvents="none" />
    </Box>
  )
}

const AddOrEdit = () => {
  const t = useT()
  const { params, back } = useRouter()
  const { id } = params
  const [user, setUser] = createStore<User>({
    id: 0,
    username: "",
    password: "",
    base_path: "",
    local_path: "",
    role: 0,
    permission: 0,
    disabled: false,
  })
  const [userLoading, loadUser] = useFetch(
    (): PResp<User> => r.get(`/admin/user/get?id=${id}`),
    id ? true : false,
  )

  const initEdit = async () => {
    const resp = await loadUser()
    handleResp<User>(resp, setUser)
  }
  if (id) {
    initEdit()
  }
  const [okLoading, ok] = useFetch((): PEmptyResp => {
    return r.post(`/admin/user/${id ? "update" : "create"}`, user)
  })

  const cardBorder = useColorModeValue("$neutral4", "$neutral6")
  const cardBg = useColorModeValue("$background", "$neutral3")

  return (
    <Box
      w="$full"
      p={{ "@initial": "$4", "@sm": "$6" }}
      rounded="$lg"
      border="1px solid"
      borderColor={cardBorder()}
      bg={cardBg()}
      shadow="$xs"
    >
      <Show
        when={!userLoading()}
        fallback={
          <VStack w="$full" alignItems="stretch" spacing="$4">
            <Skeleton w="120px" h="32px" rounded="$md" />
            <Skeleton w="$full" h="40px" rounded="$md" />
            <Skeleton w="$full" h="40px" rounded="$md" />
            <Skeleton w="$full" h="40px" rounded="$md" />
            <Skeleton w="$full" h="100px" rounded="$md" />
          </VStack>
        }
      >
        <VStack w="$full" alignItems="stretch" spacing="$4">
          <HStack w="$full" justifyContent="space-between" alignItems="center">
            <Heading size={{ "@initial": "base", "@sm": "lg" }}>
              {t(`global.${id ? "edit" : "add"}`)}
            </Heading>
            <Button variant="ghost" size="sm" onClick={() => back()}>
              {t("global.back")}
            </Button>
          </HStack>
          <FormControl w="$full" display="flex" flexDirection="column" required>
            <FormLabel for="username" display="flex" alignItems="center">
              {t(`users.username`)}
            </FormLabel>
            <Input
              id="username"
              w="$full"
              value={user.username}
              onInput={(e) => setUser("username", e.currentTarget.value)}
            />
          </FormControl>
          <FormControl w="$full" display="flex" flexDirection="column" required>
            <FormLabel for="password" display="flex" alignItems="center">
              {t(`users.password`)}
            </FormLabel>
            <Input
              id="password"
              w="$full"
              type="password"
              placeholder="********"
              value={user.password}
              onInput={(e) => setUser("password", e.currentTarget.value)}
            />
          </FormControl>

          <Show when={user.role !== UserRole.ADMIN}>
            <FormControl w="$full" display="flex" flexDirection="column" required>
              <FormLabel for="local_path" display="flex" alignItems="center">
                {t(`users.local_path`)}
              </FormLabel>
              <Input
                id="local_path"
                w="$full"
                value={user.local_path}
                placeholder="/home/ubuntu"
                onInput={(e) => setUser("local_path", e.currentTarget.value)}
              />
            </FormControl>
          </Show>
          <FormControl w="$full" required>
            <FormLabel display="flex" alignItems="center">
              {t(`users.permission`)}
            </FormLabel>
            <SimpleGrid
              columns={{ "@initial": 1, "@sm": 2, "@md": 3 }}
              gap="$2"
              w="$full"
            >
              <For each={UserPermissions}>
                {(item) => (
                  <Permission
                    name={item}
                    can={
                      ((user.permission >> UserPermissionBits[item]) & 1) === 1
                    }
                    onChange={(val) => {
                      const bit = UserPermissionBits[item]
                      if (val) {
                        setUser("permission", (user.permission |= 1 << bit))
                      } else {
                        setUser("permission", (user.permission &= ~(1 << bit)))
                      }
                    }}
                  />
                )}
              </For>
            </SimpleGrid>
          </FormControl>
          <Box
            display="inline-flex"
            alignItems="center"
            gap="$2"
            py="$1"
            cursor="pointer"
            onClick={() => setUser("disabled", !user.disabled)}
          >
            <Checkbox id="disabled" checked={user.disabled} pointerEvents="none" />
            <Text fontSize="$sm" color="$neutral11" userSelect="none">
              {t(`users.disabled`)}
            </Text>
          </Box>
          <HStack spacing="$3" w="$full" pt="$2">
            <Button
              flex={{ "@initial": 1, "@sm": "initial" }}
              loading={okLoading()}
              onClick={async () => {
                const resp = await ok()
                handleResp(resp, async () => {
                  notify.success(t("global.save_success"))
                  if (user.username === me().username)
                    handleResp(await (r.get("/me") as PResp<Me>), setMe)
                  back()
                })
              }}
            >
              {t(`global.${id ? "save" : "add"}`)}
            </Button>
            <Button
              flex={{ "@initial": 1, "@sm": "initial" }}
              variant="ghost"
              onClick={() => back()}
            >
              {t("global.back")}
            </Button>
          </HStack>
        </VStack>
      </Show>
    </Box>
  )
}

export default AddOrEdit
