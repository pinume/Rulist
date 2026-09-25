import { PreviewMeta, ProcessedContent } from "~/types"
import { CodePreview } from "./CodePreview"

export const XmlPreview = (props: {
  content?: ProcessedContent
  meta?: PreviewMeta
}) => {
  return <CodePreview content={props.content} meta={props.meta} />
}
