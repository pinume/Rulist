import { Box, useColorModeValue } from "@hope-ui/solid"

const LoginBg = () => {
  const bgColor = useColorModeValue("#f2f6fa", "#111827")
  const topGlow = useColorModeValue(
    "rgba(125, 181, 255, 0.22)",
    "rgba(59, 130, 246, 0.14)",
  )
  const bottomGlow = useColorModeValue(
    "rgba(79, 211, 190, 0.16)",
    "rgba(45, 212, 191, 0.10)",
  )
  return (
    <Box
      bgColor={bgColor()}
      pos="fixed"
      top="0"
      left="0"
      w="100vw"
      h="100vh"
      overflow="hidden"
      zIndex="$hide"
      pointerEvents="none"
    >
      <Box
        pos="absolute"
        w="48vw"
        h="48vw"
        minW="360px"
        minH="360px"
        rounded="$full"
        right="-18vw"
        top="-22vw"
        bgColor={topGlow()}
        style={{ filter: "blur(28px)" }}
      />
      <Box
        pos="absolute"
        w="42vw"
        h="42vw"
        minW="320px"
        minH="320px"
        rounded="$full"
        left="-18vw"
        bottom="-20vw"
        bgColor={bottomGlow()}
        style={{ filter: "blur(28px)" }}
      />
    </Box>
  )
}

export default LoginBg
