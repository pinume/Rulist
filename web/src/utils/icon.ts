import {
  BsFileEarmarkWordFill,
  BsFileEarmarkExcelFill,
  BsFileEarmarkPptFill,
  BsFileEarmarkPdfFill,
  BsFileEarmarkPlayFill,
  BsFileEarmarkMusicFill,
  BsFileEarmarkFontFill,
  BsFileEarmarkImageFill,
  BsFileEarmarkMinusFill,
  BsApple,
  BsWindows,
  BsFileEarmarkZipFill,
  BsMarkdownFill,
  BsFileEarmarkTextFill,
  BsFileEarmarkCodeFill,
} from "solid-icons/bs"
import {
  FaSolidDatabase,
  FaSolidBook,
  FaSolidCompactDisc,
  FaSolidLink,
} from "solid-icons/fa"
import { IoFolder } from "solid-icons/io"
import { ImAndroid } from "solid-icons/im"
import { Obj, ObjType } from "~/types"
import { ext } from "./path"
import {
  VscodeIconsFileTypeAi2,
  VscodeIconsFileTypePhotoshop2,
} from "~/components"
import { SiAsciinema } from "solid-icons/si"
import { FiGlobe } from "solid-icons/fi"
import { getMainColor } from "~/store"

const iconMap = {
  "zip,tar,gz,tgz,bz2,xz,7z,rar": BsFileEarmarkZipFill,
  "dmg,ipa,plist,tipa": BsApple,
  "exe,msi": BsWindows,
  apk: ImAndroid,
  db: FaSolidDatabase,
  "md,markdown": BsMarkdownFill,
  epub: FaSolidBook,
  iso: FaSolidCompactDisc,
  m3u8: BsFileEarmarkPlayFill,
  "doc,docx,wps,rtf,odt,dot,dotx": BsFileEarmarkWordFill,
  "xls,xlsx,csv,tsv,et,xlt,xltx,xlsm": BsFileEarmarkExcelFill,
  "ppt,pptx,pps,ppsx,dps,key,pot,potx,pptm": BsFileEarmarkPptFill,
  pdf: BsFileEarmarkPdfFill,
  "txt,log,text": BsFileEarmarkTextFill,
  "js,ts,jsx,tsx,json,xml,yaml,yml,rs,go,py,c,cpp,h,sh,toml,sql,css,scss":
    BsFileEarmarkCodeFill,
  psd: VscodeIconsFileTypePhotoshop2,
  ai: VscodeIconsFileTypeAi2,
  url: FaSolidLink,
  cast: SiAsciinema,
  "html,htm": FiGlobe,
  "jpg,jpeg,png,gif,bmp,webp,svg,ico,tiff,heic": BsFileEarmarkImageFill,
  "mp4,mkv,avi,mov,wmv,flv,webm,m4v,rmvb,ts": BsFileEarmarkPlayFill,
  "mp3,flac,ogg,m4a,wav,opus,aac,wma": BsFileEarmarkMusicFill,
}

export const getIconByTypeAndName = (type: number, name: string) => {
  if (type !== ObjType.FOLDER) {
    for (const [extensions, icon] of Object.entries(iconMap)) {
      if (extensions.split(",").includes(ext(name).toLowerCase())) {
        return icon
      }
    }
  }
  switch (type) {
    case ObjType.FOLDER:
      return IoFolder
    case ObjType.VIDEO:
      return BsFileEarmarkPlayFill
    case ObjType.AUDIO:
      return BsFileEarmarkMusicFill
    case ObjType.TEXT:
      return BsFileEarmarkFontFill
    case ObjType.IMAGE:
      return BsFileEarmarkImageFill
    default:
      return BsFileEarmarkMinusFill
  }
}

export const getIconByObj = (obj: Pick<Obj, "type" | "name">) => {
  return getIconByTypeAndName(obj.type, obj.name)
}

export const getIconColorByObj = (obj: Pick<Obj, "type" | "name">) => {
  if (obj.type === ObjType.FOLDER) return getMainColor()
  const name = obj.name.toLowerCase()
  if (/\.(zip|tar|gz|tgz|bz2|xz|7z|rar)$/.test(name)) return "#d97706"
  if (/\.(html|htm)$/.test(name)) return "#0284c7"
  if (/\.(doc|docx|wps|rtf|odt|dot|dotx)$/.test(name)) return "#185abd"
  if (/\.(ppt|pptx|pps|ppsx|dps|key|pot|potx|pptm)$/.test(name)) return "#d24726"
  if (/\.(xls|xlsx|csv|tsv|et|xlt|xltx|xlsm)$/.test(name)) return "#107c41"
  if (/\.pdf$/.test(name)) return "#dc2626"
  if (/\.(txt|log|text)$/.test(name)) return "#475569"
  if (/\.(md|markdown)$/.test(name)) return "#2563eb"
  if (
    /\.(js|ts|jsx|tsx|json|xml|yaml|yml|rs|go|py|c|cpp|h|sh|toml|sql|css|scss)$/.test(
      name,
    )
  )
    return "#0d9488"
  if (
    obj.type === ObjType.IMAGE ||
    /\.(jpg|jpeg|png|gif|bmp|webp|svg|ico|tiff|heic)$/.test(name)
  )
    return "#8b5cf6"
  if (
    obj.type === ObjType.VIDEO ||
    /\.(mp4|mkv|avi|mov|wmv|flv|webm|m4v|rmvb|ts)$/.test(name)
  )
    return "#ef4444"
  if (
    obj.type === ObjType.AUDIO ||
    /\.(mp3|flac|ogg|m4a|wav|opus|aac|wma)$/.test(name)
  )
    return "#10b981"
  return "$neutral10"
}
