import { useTitle } from "~/hooks"
import { config } from "~/store"
import { notify } from "~/utils"
import { Body } from "./Body"
import { Header } from "./header/Header"
import { Toolbar } from "./toolbar/Toolbar"

const Index = () => {
  useTitle(() => config()?.site_title || "Rulist")
  const announcement = config()?.announcement
  if (announcement) {
    notify.info(announcement)
  }
  return (
    <>
      <Header />
      <Toolbar />
      <Body />
    </>
  )
}

export default Index
