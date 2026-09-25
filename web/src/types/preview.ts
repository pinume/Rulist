export type PreviewType =
  | "image"
  | "video"
  | "audio"
  | "pdf"
  | "markdown"
  | "text"
  | "code"
  | "json"
  | "xml"
  | "csv"
  | "archive"
  | "html"
  | "unknown"

export type PreviewStrategy = "direct" | "processed" | "unsupported"

export interface PreviewMeta {
  name: string
  path: string
  size: number
  modified: string
  mime_type: string
  preview_type: PreviewType
  strategy: PreviewStrategy
  raw_url: string
  permissions?: string
}

export interface ProcessedContent {
  kind: string
  value: string
}

export interface PreviewResponse {
  meta: PreviewMeta
  content?: ProcessedContent
  error?: string
}

