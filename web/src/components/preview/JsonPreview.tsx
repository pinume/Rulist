import { PreviewMeta, ProcessedContent } from "~/types"
import { CodePreview } from "./CodePreview"

export const JsonPreview = (props: {
  content?: ProcessedContent
  meta?: PreviewMeta
}) => {
  return <CodePreview content={props.content} meta={props.meta} />
}
