import {
  Badge,
  Box,
  Button,
  css,
  Flex,
  HStack,
  Skeleton,
  Table,
  Tbody,
  Td,
  Text,
  Th,
  Thead,
  Tooltip,
  Tr,
  useColorModeValue,
  VStack,
} from "@hope-ui/solid"
import { createSignal, For, Show } from "solid-js"
import {
  useFetch,
  useListFetch,
  useManageTitle,
  useRouter,
  useT,
} from "~/hooks"
import { handleResp, notify, r } from "~/utils"
import {
  UserPermissions,
  UserPermissionBits,
  User,
  UserMethods,
  PPageResp,
  PEmptyResp,
} from "~/types"
import { DeletePopover } from "../common/DeletePopover"

const mobileOnlyClass = css({
  display: "block",
  width: "100%",
  "@md": {
    display: "none",
  },
})

const desktopOnlyClass = css({
  display: "none",
  width: "100%",
  "@md": {
    display: "block",
  },
})

const Role = (props: { role: number }) => {
  const roles = [
    { name: "general", label: "普通用户", color: "info" },
    null,
    { name: "admin", label: "管理员", color: "accent" },
  ]
  const role = roles[props.role] ?? roles[0]!
  return <Badge colorScheme={role.color as any}>{role.label}</Badge>
}

const Permissions = (props: { user: User }) => {
  const t = useT()
  const color = (can: boolean) => `$${can ? "success" : "danger"}9`
  return (
    <HStack spacing="$0_5">
      <For each={UserPermissions}>
        {(item) => (
          <Tooltip label={t(`users.permissions.${item}`)}>
            <Box
              boxSize="$2"
              rounded="$full"
              bg={color(UserMethods.can(props.user, UserPermissionBits[item]))}
            ></Box>
          </Tooltip>
        )}
      </For>
    </HStack>
  )
}

const MobilePermissions = (props: { user: User }) => {
  const t = useT()
  const granted = () =>
    UserPermissions.filter((item) =>
      UserMethods.can(props.user, UserPermissionBits[item]),
    )
  return (
    <Flex wrap="wrap" gap="$1">
      <For
        each={granted()}
        fallback={
          <Text fontSize="$xs" color="$neutral9">
            无特定权限
          </Text>
        }
      >
        {(item) => (
          <Badge colorScheme="info" variant="subtle">
            {t(`users.permissions.${item}`)}
          </Badge>
        )}
      </For>
    </Flex>
  )
}

const Users = () => {
  const t = useT()
  useManageTitle("manage.sidemenu.users")
  const { to } = useRouter()
  const [getUsersLoading, getUsers] = useFetch((): PPageResp<User> =>
    r.get("/admin/user/list"),
  )
  const [users, setUsers] = createSignal<User[]>([])
  const [loaded, setLoaded] = createSignal(false)
  const refresh = async () => {
    const resp = await getUsers()
    handleResp(resp, (data) => setUsers(data.content))
    setLoaded(true)
  }
  refresh()

  const [deleting, deleteUser] = useListFetch((id: number): PEmptyResp =>
    r.post(`/admin/user/delete?id=${id}`),
  )
  const [cancel_2faId, cancel_2fa] = useListFetch((id: number): PEmptyResp =>
    r.post(`/admin/user/cancel_2fa?id=${id}`),
  )

  const cardBorder = useColorModeValue("$neutral4", "$neutral6")
  const cardBg = useColorModeValue("$background", "$neutral3")
  const dividerColor = useColorModeValue("$neutral4", "$neutral6")

  return (
    <VStack spacing="$3" alignItems="stretch" w="$full">
      <HStack spacing="$2" w="$full" justifyContent="space-between">
        <HStack spacing="$2">
          <Button
            colorScheme="accent"
            size={{ "@initial": "sm", "@sm": "md" }}
            loading={getUsersLoading()}
            onClick={refresh}
          >
            {t("global.refresh")}
          </Button>
          <Button
            size={{ "@initial": "sm", "@sm": "md" }}
            onClick={() => {
              to("/@settings/users/add")
            }}
          >
            {t("global.add")}
          </Button>
        </HStack>
      </HStack>

      {/* 移动端卡片视图 */}
      <Box class={mobileOnlyClass()} w="$full">
        <Show
          when={loaded()}
          fallback={
            <VStack spacing="$3" w="$full" alignItems="stretch">
              <Skeleton w="$full" h="140px" rounded="$lg" />
              <Skeleton w="$full" h="140px" rounded="$lg" />
            </VStack>
          }
        >
          <VStack spacing="$3" w="$full" alignItems="stretch">
            <For
              each={users()}
              fallback={
                <Box p="$6" textAlign="center" color="$neutral10" w="$full">
                  暂无用户
                </Box>
              }
            >
            {(user) => (
              <Box
                p="$3_5"
                rounded="$lg"
                border="1px solid"
                borderColor={cardBorder()}
                bg={cardBg()}
                shadow="$xs"
              >
                {/* 头部：用户名、角色与启用状态 */}
                <HStack
                  justifyContent="space-between"
                  alignItems="center"
                  mb="$2"
                >
                  <HStack spacing="$2" alignItems="center" minW={0}>
                    <Text fontWeight="$bold" fontSize="$md" noOfLines={1}>
                      {user.username}
                    </Text>
                    <Role role={user.role} />
                  </HStack>
                  <Badge
                    colorScheme={user.disabled ? "danger" : "success"}
                    variant="subtle"
                  >
                    {user.disabled ? t("users.disabled") : t("users.available")}
                  </Badge>
                </HStack>

                {/* 服务器目录 */}
                <VStack spacing="$0_5" alignItems="start" mb="$2">
                  <Text fontSize="$xs" color="$neutral10">
                    {t("users.local_path")}
                  </Text>
                  <Text
                    fontSize="$sm"
                    css={{ wordBreak: "break-all" }}
                    color={user.local_path ? "$neutral12" : "$neutral9"}
                  >
                    {user.local_path || "—"}
                  </Text>
                </VStack>

                {/* 权限列表 */}
                <VStack spacing="$1" alignItems="start" mb="$3">
                  <Text fontSize="$xs" color="$neutral10">
                    {t("users.permission")}
                  </Text>
                  <MobilePermissions user={user} />
                </VStack>

                {/* 操作按钮栏 */}
                <HStack
                  spacing="$2"
                  pt="$2"
                  borderTop="1px solid"
                  borderColor={dividerColor()}
                >
                  <Button
                    flex={1}
                    size="sm"
                    variant="subtle"
                    onClick={() => {
                      to(`/@settings/users/edit/${user.id}`)
                    }}
                  >
                    {t("global.edit")}
                  </Button>
                  <Button
                    flex={1}
                    size="sm"
                    colorScheme="accent"
                    variant="subtle"
                    loading={cancel_2faId() === user.id}
                    onClick={async () => {
                      const resp = await cancel_2fa(user.id)
                      handleResp(resp, () => {
                        notify.success(t("users.cancel_2fa_success"))
                        refresh()
                      })
                    }}
                  >
                    {t("users.cancel_2fa")}
                  </Button>
                  <DeletePopover
                    flex={1}
                    size="sm"
                    w="$full"
                    name={user.username}
                    loading={deleting() === user.id}
                    onClick={async () => {
                      const resp = await deleteUser(user.id)
                      handleResp(resp, () => {
                        notify.success(t("global.delete_success"))
                        refresh()
                      })
                    }}
                  />
                </HStack>
              </Box>
            )}
          </For>
        </VStack>
        </Show>
      </Box>

      {/* 桌面端表格视图 */}
      <Box class={desktopOnlyClass()} w="$full" overflowX="auto">
        <Show
          when={loaded()}
          fallback={
            <VStack spacing="$2" w="$full" alignItems="stretch">
              <Skeleton w="$full" h="44px" rounded="$md" />
              <Skeleton w="$full" h="44px" rounded="$md" />
              <Skeleton w="$full" h="44px" rounded="$md" />
            </VStack>
          }
        >
          <Table highlightOnHover dense>
          <Thead>
            <Tr>
              <For
                each={[
                  "username",
                  "local_path",
                  "role",
                  "permission",
                  "available",
                ]}
              >
                {(title) => <Th>{t(`users.${title}`)}</Th>}
              </For>
              <Th>{t("global.operations")}</Th>
            </Tr>
          </Thead>
          <Tbody>
            <For each={users()}>
              {(user) => (
                <Tr>
                  <Td>{user.username}</Td>
                  <Td>{user.local_path || "—"}</Td>
                  <Td>
                    <Role role={user.role} />
                  </Td>
                  <Td>
                    <Permissions user={user} />
                  </Td>
                  <Td>
                    <Badge colorScheme={!user.disabled ? "success" : "danger"}>
                      {t(`global.${!user.disabled ? "yes" : "no"}`)}
                    </Badge>
                  </Td>
                  <Td>
                    <HStack spacing="$2">
                      <Button
                        size="sm"
                        onClick={() => {
                          to(`/@settings/users/edit/${user.id}`)
                        }}
                      >
                        {t("global.edit")}
                      </Button>
                      <DeletePopover
                        size="sm"
                        name={user.username}
                        loading={deleting() === user.id}
                        onClick={async () => {
                          const resp = await deleteUser(user.id)
                          handleResp(resp, () => {
                            notify.success(t("global.delete_success"))
                            refresh()
                          })
                        }}
                      />
                      <Button
                        size="sm"
                        colorScheme="accent"
                        loading={cancel_2faId() === user.id}
                        onClick={async () => {
                          const resp = await cancel_2fa(user.id)
                          handleResp(resp, () => {
                            notify.success(t("users.cancel_2fa_success"))
                            refresh()
                          })
                        }}
                      >
                        {t("users.cancel_2fa")}
                      </Button>
                    </HStack>
                  </Td>
                </Tr>
              )}
            </For>
          </Tbody>
        </Table>
        </Show>
      </Box>
    </VStack>
  )
}

export default Users
