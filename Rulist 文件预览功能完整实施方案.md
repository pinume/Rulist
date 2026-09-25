# Rulist 文件预览功能完整实施方案

## 1. 项目目标

为 Rulist 增加统一、安全、高性能、可扩展的文件预览能力。

核心目标：

- 常见文件点击后直接预览。
- 浏览器原生能处理的文件尽量交给浏览器。
- 服务端只处理确实需要解析或转换的格式。
- 不影响目录浏览速度。
- 不对全部文件执行 MIME 深度检测或 Magika。
- 大文件不得无上限读入内存。
- 轻量预览和重型预览均有并发保护。
- 预览失败不得影响文件下载和 Raw 访问。
- 缓存不得绕过权限验证。
- Signed URL 与 Preview Cache 生命周期必须解耦。
- 后端通过 `StorageManager` 访问文件，不直接依赖 `LocalDriver`。
- 后续接入 S3、WebDAV 等 Driver 时无需重写 Preview 核心。
- 前端继续使用 SolidJS 和 Rulist 原有字体体系。

整体设计原则：

> Preview 是文件访问能力的增强层，而不是文件访问的前置条件。

---

# 2. 可借鉴项目的职责划分

本方案综合三个方向。

## Peekd

借鉴：

- Raw 文件与 Preview 页面分离。
- 图片、音视频、PDF 优先使用浏览器原生能力。
- Text/Markdown/JSON/CSV 等按类型处理。
- Preview 失败后可靠 fallback。

不照搬：

- Go Template 前端。
- 完全依赖本地文件系统的设计。

## Yazi

借鉴：

- Previewer / Preloader 分离。
- Preview Signature。
- Preview Cache。
- 并发 Worker。
- Cancellation。
- Singleflight。
- 大文件 seek / cursor 思路。

不照搬：

- PDF 转图片作为正文预览。
- 视频截图代替视频播放。
- 为终端显示而存在的转换逻辑。

## Magika

只用于：

- 无扩展名文件。
- 未知扩展名。
- `.bin`、`.dat` 等模糊类型。
- 后续需要的扩展名冲突检测。

Magika 不参与普通目录浏览，也不作为 Rulist Preview 的基础依赖。

---

# 3. Rulist 当前可直接复用的能力

当前 Rulist 已经具备完整的 Raw 文件通路，因此不需要重新设计 `?raw=1`。

`fs_list` 和 `fs_get` 当前都会为文件生成带签名的 `raw_url`。

Raw Preview 当前已经支持：

```text
HTTP Range
206 Partial Content
Accept-Ranges
Content-Range
Content-Length
inline Content-Disposition
```

因此已经可以支撑：

```text
Video
Audio
PDF
大型文件流式读取
播放进度拖动
```



Raw Preview 在 inline 模式下还已经设置：

```http
Content-Security-Policy: sandbox
```

可继续作为不可信内容预览的后端安全基础。

当前前端：

```text
web/src/pages/home/file/File.tsx
```

仍然只是显示文件名、文件大小和下载按钮，非常适合作为 Preview 入口直接替换。

---

# 4. 总体架构

```text
                         File

                          │
                          ▼
                FileTypeDetector
                          │
              ┌───────────┼────────────┐
              │           │            │
          Extension   TypeCache      Magika
                                      后期
                          │
                          ▼
                 PreviewResolver
                          │
             ┌────────────┴─────────────┐
             │                          │
             ▼                          ▼

      Direct Preview              Processed Preview

   Image / Video                 Markdown / Text
   Audio / PDF                   Code / JSON
   HTML / SVG                    XML / CSV
                                 Archive

             │                          │
             │                          ▼
             │                 PreviewProcessor
             │                          │
             │               ┌──────────┴──────────┐
             │               │                     │
             │          Lightweight              Heavy
             │               │                     │
             │          Semaphore            PreviewRuntime
             │                                     │
             │                              ├─ Worker Limit
             │                              ├─ Cancellation
             │                              ├─ Singleflight
             │                              └─ Cache
             │
             └──────────────────┬──────────────────┘
                                │
                                ▼
                         Preview Response
                                │
                                ▼
                       FilePreviewLayout
```

---

# 5. Preview 模块职责

后端最终建议：

```text
src/
└── preview/
    ├── mod.rs
    ├── types.rs
    ├── detector.rs
    ├── resolver.rs
    ├── limits.rs
    │
    ├── processor/
    │   ├── mod.rs
    │   ├── text.rs
    │   ├── markdown.rs
    │   ├── json.rs
    │   ├── xml.rs
    │   ├── csv.rs
    │   └── archive.rs
    │
    ├── runtime.rs       # Phase 4
    ├── cache.rs         # Phase 5
    └── signature.rs     # Phase 5
```

服务接口：

```text
src/server/preview.rs
```

前端：

```text
web/src/components/preview/
```

---

# 6. PreviewType

统一定义：

```rust
pub enum PreviewType {
    Image,
    Video,
    Audio,
    Pdf,

    Markdown,
    Text,
    Code,
    Json,
    Xml,
    Csv,

    Archive,
    Html,

    Unknown,
}
```

前端只认 PreviewType。

例如：

```text
.jpg
.jpeg
image/jpeg
```

统一：

```text
Image
```

---

# 7. PreviewStrategy

增加：

```rust
pub enum PreviewStrategy {
    Direct,
    Processed,
    Unsupported,
}
```

对应：

```text
Image      → Direct
Video      → Direct
Audio      → Direct
PDF        → Direct
HTML       → Direct

Markdown   → Processed
Text       → Processed
Code       → Processed
JSON       → Processed
XML        → Processed
CSV        → Processed
Archive    → Processed

Unknown    → Unsupported
```

核心要求：

> Direct Preview 不进入 PreviewJob，也不进入 Processor。

---

# 8. FileTypeDetector

## 8.1 第一阶段：扩展名

首先只使用扩展名。

例如：

```text
jpg/jpeg/png/gif/webp/avif/svg → Image

mp4/webm/mov/mkv               → Video

mp3/flac/ogg/opus/wav/m4a      → Audio

pdf                            → Pdf

md/markdown                    → Markdown

txt/log/conf/ini/properties    → Text

rs/go/py/js/ts/tsx/jsx
c/h/cpp/java/css/sh/sql        → Code

json/jsonc                     → Json

xml                            → Xml

csv/tsv                        → Csv

html/htm                       → Html

zip/tar/tgz/tar.gz             → Archive
```

其他：

```text
Unknown
```

---

# 9. Magika 策略

Magika 放到 Phase 6。

最终顺序：

```text
Extension
    │
    ├── Known
    │     ↓
    │ PreviewType
    │
    └── Unknown
          ↓
       TypeCache
          │
      ┌───┴────┐
     Hit      Miss
      │         │
      │       Magika
      │         │
      └────┬────┘
           ↓
      PreviewType
```

不得执行：

```text
打开目录
→ 扫描每一个文件
→ Magika
```

允许执行：

```text
用户打开 unknown.bin
→ Magika
```

模型只初始化一次，不允许每次请求重新加载。

---

# 10. PreviewMeta

统一返回：

```rust
pub struct PreviewMeta {
    pub name: String,
    pub path: String,

    pub size: u64,
    pub modified: String,

    pub mime_type: String,

    pub preview_type: PreviewType,
    pub strategy: PreviewStrategy,

    pub raw_url: String,
}
```

例如：

```json
{
  "name": "README.md",
  "path": "/docs/README.md",
  "size": 28123,
  "modified": "2026-09-25T20:30:00Z",
  "mime_type": "text/markdown",
  "preview_type": "markdown",
  "strategy": "processed",
  "raw_url": "/p/docs/README.md?sign=..."
}
```

---

# 11. Preview API

第一阶段只新增：

```text
POST /api/fs/preview
```

请求：

```json
{
  "path": "/docs/README.md"
}
```

Direct Preview：

```json
{
  "meta": {
    "preview_type": "video",
    "strategy": "direct",
    "raw_url": "..."
  },
  "content": null
}
```

Processed：

```json
{
  "meta": {
    "preview_type": "markdown",
    "strategy": "processed",
    "raw_url": "..."
  },
  "content": {
    "kind": "html",
    "value": "..."
  }
}
```

Unknown：

```json
{
  "meta": {
    "preview_type": "unknown",
    "strategy": "unsupported",
    "raw_url": "..."
  },
  "content": null
}
```

---

# 12. 权限执行顺序

任何 Preview 请求必须：

```text
Request
   ↓
authenticate_user
   ↓
user_path
   ↓
Permission check
   ↓
StorageManager
   ↓
PreviewCache lookup
   ↓
PreviewProcessor
```

绝对禁止：

```text
Request
   ↓
PreviewCache Hit
   ↓
直接返回
```

因为：

> Cache 缓存的是文件预览结果，不是用户访问权限。

任何 Cache Hit 都不能绕过鉴权。

---

# 13. StorageManager 边界

Preview 模块只能使用：

```rust
state.storage.get(path)
state.storage.open(path)
state.storage.list(path)
```

不能：

```rust
LocalDriver::open(...)
std::fs::File::open(...)
```

当前 `StorageManager` 已经集中提供 `get/open/list` 等入口。

这保证以后：

```text
Local
S3
WebDAV
OSS
```

可以继续使用同一 Preview API。

---

# 14. Direct Preview

## Image

```tsx
<img src={rawUrl} />
```

支持浏览器原生格式。

SVG：

```text
只通过 <img>
禁止 innerHTML
禁止 inline SVG
```

---

## Video

```tsx
<video
  src={rawUrl}
  controls
  preload="metadata"
/>
```

Rulist 现有 Range 能力直接复用。

服务器不转码。

---

## Audio

```tsx
<audio
  src={rawUrl}
  controls
  preload="metadata"
/>
```

---

## PDF

```tsx
<iframe
  src={rawUrl}
  title={name}
/>
```

第一版：

```text
不引入 PDF.js
不使用 pdftoppm 作为正文预览
```

未来 pdftoppm 只考虑 PDF 缩略图。

---

# 15. HTML / SVG 安全

HTML 必须：

```tsx
<iframe
  src={rawUrl}
  sandbox=""
/>
```

明确规定：

```text
不得添加 allow-same-origin
不得添加 allow-scripts
不得添加 allow-top-navigation
不得添加 allow-forms
不得添加 allow-popups
```

特别是：

> 不允许为了改善 HTML 页面兼容性而随意加入 `allow-same-origin`。

Raw Preview 后端已有 CSP sandbox，可以形成双层保护。

SVG：

```tsx
<img src={rawUrl} />
```

禁止：

```text
fetch SVG
→ innerHTML
```

---

# 16. Markdown Preview

流程：

```text
File
 ↓
Size check
 ↓
UTF-8 check
 ↓
Markdown Parser
 ↓
HTML Sanitizer
 ↓
MarkdownPreview
```

推荐：

```text
pulldown-cmark
ammonia
```

第一版支持：

```text
标题
列表
表格
任务列表
链接
图片
引用
代码块
```

暂不加入 Mermaid。

重要规则：

> Markdown Parser 的输出不能直接进入 DOM，必须经过 Sanitizer。

---

# 17. Text / Code

Phase 2 初版：

```text
MAX_TEXT_PREVIEW_SIZE = 4 MiB
```

超过：

```text
too_large
```

不能继续整体读取。

编码：

```text
UTF-8 → Preview
非 UTF-8 → unsupported_encoding
```

Code 第一阶段：

```text
行号
复制
Raw
下载
横向滚动
```

不引入 Monaco Editor。

---

# 18. JSON / XML

## JSON

```text
serde_json parse
 ↓
pretty format
 ↓
Code Viewer
```

失败：

```text
fallback → Text
```

不能返回 500。

## XML

Phase 2 可以先作为 Text/Code 显示。

后续再使用：

```text
quick-xml
```

进行 pretty format。

解析失败：

```text
fallback → Text
```

---

# 19. Processed Preview 并发保护

并发保护必须从 Phase 2 就开始。

新增一个全局：

```rust
tokio::sync::Semaphore
```

建议默认：

```text
processed_preview_concurrency = 8
```

以下操作都必须先获取 permit：

```text
Markdown
Text
Code
JSON
XML
CSV
Archive metadata scan
```

流程：

```text
Request
 ↓
Permission
 ↓
Semaphore.acquire()
 ↓
Processor
 ↓
Permit released
```

这样大量并发请求不会同时占满 CPU。

Phase 4 再升级成分类 Worker。

---

# 20. CSV / TSV

Phase 3 加入。

推荐：

```text
csv crate
```

返回：

```json
{
  "kind": "table",
  "columns": [],
  "rows": []
}
```

Phase 3 初版限制：

```text
max_csv_size    = 4 MiB
max_csv_rows    = 500
max_csv_columns = 200
```

前端必须：

```text
横向滚动
固定表头
限制单元格最大宽度
长文本截断
```

不得一次渲染几十万行。

---

# 21. Archive Preview

Phase 3 支持：

```text
ZIP
TAR
TAR.GZ
```

只扫描目录 metadata。

返回：

```text
name
size
modified
is_dir
```

禁止：

```text
自动解压到磁盘
自动读取所有文件内容
```

---

# 22. Archive 安全限制

必须设置：

```text
max_archive_entries              = 10_000
max_archive_name_length          = 1_024
max_archive_total_uncompressed   = 10 GiB
```

ZIP 可以增加：

```text
max_compression_ratio = 1000
```

计算：

```text
uncompressed_size / compressed_size
```

未来如果支持“压缩包内部文件预览”，还必须增加：

```text
max_entry_uncompressed_size
max_actual_output_bytes
```

其中：

> `max_actual_output_bytes` 才是最终防止 ZIP bomb 的硬限制。

不能只相信 ZIP Header 声明的大小。

---

# 23. 大文件 Preview

Phase 2/3：

```text
<= 4 MiB
→ 完整文本处理

> 4 MiB
→ too_large
```

Phase 4 再加入：

```text
Windowed Preview
```

例如：

```text
2 GB server.log
```

只能读取：

```text
当前窗口
```

而不是整个文件。

---

# 24. Opaque Cursor

不要定义统一数字型 cursor。

API 使用：

```json
{
  "cursor": "opaque-token"
}
```

响应：

```json
{
  "content": "...",
  "next_cursor": "...",
  "previous_cursor": "..."
}
```

内部可以代表：

```text
Text      → byte/line offset
CSV       → row offset
Archive   → entry index
PDF       → page
Ebook     → chapter
```

前端不需要理解 cursor 内部格式。

---

# 25. PreviewRuntime

Phase 4 再建立：

```text
PreviewRuntime
├── normal semaphore
├── heavy semaphore
├── Cancellation
└── Singleflight
```

建议：

```text
normal_preview_workers = 8
heavy_preview_workers  = 2
```

普通：

```text
Markdown
JSON
CSV
Archive metadata
```

Heavy：

```text
ffmpeg
PDF thumbnail
HEIC conversion
Office conversion
```

---

# 26. Cancellation

前端首先使用：

```ts
AbortController
```

用户：

```text
打开 A.md
立刻点击 B.pdf
```

应该取消 A 的请求。

Phase 4 后端重型任务再增加真正的：

```text
CancellationToken
JoinHandle::abort()
```

规则：

```text
旧Preview不再需要
→ 尽快停止CPU/IO任务
```

---

# 27. Singleflight

同一个 PreviewSignature 同时出现：

```text
video.mp4 poster
video.mp4 poster
video.mp4 poster
```

只能：

```text
创建1个Job
其他请求等待结果
```

不能：

```text
启动3个ffmpeg
```

---

# 28. PreviewSignature

Phase 5：

```rust
PreviewSignature {
    path,
    size,
    modified,
    preview_type,
    variant,
    width,
    height,
}
```

Hash 后作为 Preview Artifact Key。

文件：

```text
大小变化
mtime变化
Preview参数变化
```

缓存自动失效。

---

# 29. PreviewCache

只缓存生成结果。

缓存：

```text
Video poster
PDF thumbnail
HEIC → WebP
Office preview
Markdown processed result（可选）
```

不缓存：

```text
原始图片
原始视频
原始音频
PDF原文件
signed raw_url
用户权限结果
```

建议：

```text
preview_cache_max_size = 1 GiB
```

超过后按：

```text
LRU / last_accessed
```

清理。

---

# 30. Signed raw_url 策略

当前 Rulist：

```rust
DEFAULT_LIFETIME_SECS = 300
```

也就是默认有效期 **5 分钟**。

因此必须明确：

> Signed `raw_url` 永远不能进入长期 PreviewCache。

正确流程：

```text
Request
 ↓
Authenticate
 ↓
PreviewCache Hit
 ↓
取出缓存的处理结果
 ↓
现场调用 sign_path()
 ↓
生成新的5分钟raw_url
 ↓
返回PreviewResponse
```

PreviewCache 中允许存：

```text
preview_type
mime_type
processed_content
artifact_key
source_signature
```

不允许存：

```text
raw_url
sign
用户权限结果
```

---

# 31. Symlink 安全

当前 Rulist `LocalDriver::safe_resolve()` 已经逐级检查 symbolic link，只要任一组件是 symlink 就直接拒绝。

`open()` 也会先经过 `safe_resolve()`。

因此 Preview 必须继续通过：

```text
StorageManager
→ LocalDriver.safe_resolve()
```

不能绕过。

必须增加回归测试：

```text
symlink file → root外文件
symlink dir  → root外目录
symlink      → root内其他位置
nested symlink
broken symlink
```

全部拒绝。

---

# 32. 前端组件结构

```text
web/src/components/preview/
├── FilePreviewLayout.tsx
├── PreviewHeader.tsx
├── PreviewLoading.tsx
├── PreviewError.tsx
├── UnsupportedPreview.tsx
│
├── ImagePreview.tsx
├── VideoPreview.tsx
├── AudioPreview.tsx
├── PdfPreview.tsx
├── HtmlPreview.tsx
│
├── MarkdownPreview.tsx
├── TextPreview.tsx
├── CodePreview.tsx
├── JsonPreview.tsx
├── XmlPreview.tsx
├── CsvPreview.tsx
└── ArchivePreview.tsx
```

原：

```text
web/src/pages/home/file/File.tsx
```

只负责：

```text
请求 Preview API
 ↓
Loading/Error
 ↓
根据 PreviewType 选择 Viewer
```

不要把各类预览逻辑继续堆在 `File.tsx`。

---

# 33. FilePreviewLayout

统一结构：

```text
Home / Documents / README.md

┌──────────────────────────────────────┐
│ README.md                  下载   ⋯  │
│ Markdown · 28 KB · 2026-09-25       │
├──────────────────────────────────────┤
│                                      │
│             Preview Area             │
│                                      │
└──────────────────────────────────────┘
```

视觉延续已经确定的 Rulist 方向：

```text
Rulist原有字体
Peekd式克制视觉
冷灰页面背景
白色Preview容器
浅边框
轻阴影
12～14px圆角
Rulist蓝作为重点色
```

---

# 34. 错误状态统一

定义：

```text
unsupported
too_large
unsupported_encoding
parse_error
resource_limit
cancelled
permission_denied
internal_error
```

前端统一处理。

例如：

```text
无法预览此文件

文件格式暂不支持。

[下载] [打开原文件]
```

---

# 35. Fallback 规则

统一：

```text
Markdown parse error
→ Text

JSON parse error
→ Text

XML parse error
→ Text

CSV parse error
→ Text / Unsupported

Archive parse error
→ Raw / Download

Magika error
→ Unknown

Heavy Preview error
→ Raw / Download

Preview Cache error
→ 重新生成或Direct

任何Preview错误
→ 不影响Download
```

---

# 36. 配置默认值

建议集中在：

```text
PreviewLimits
```

默认：

```text
processed_preview_concurrency      = 8

max_text_preview_size              = 4 MiB

max_csv_size                       = 4 MiB
max_csv_rows                       = 500
max_csv_columns                    = 200

max_archive_entries                = 10_000
max_archive_name_length            = 1_024
max_archive_total_uncompressed     = 10 GiB
max_archive_compression_ratio      = 1000

normal_preview_workers             = 8
heavy_preview_workers              = 2

preview_cache_max_size             = 1 GiB
```

---

# 37. Phase 1 — 基础框架与 Direct Preview

新增：

```text
src/preview/mod.rs
src/preview/types.rs
src/preview/detector.rs
src/preview/resolver.rs
src/server/preview.rs
```

完成：

```text
PreviewType
PreviewStrategy
PreviewMeta
Extension Detector
PreviewResolver
/api/fs/preview
```

支持：

```text
Image
Video
Audio
PDF
HTML
Unknown
```

前端建立：

```text
FilePreviewLayout
ImagePreview
VideoPreview
AudioPreview
PdfPreview
HtmlPreview
UnsupportedPreview
```

### Phase 1 验收

```text
.jpg       正常显示
.mp4       正常播放并可拖动
.mp3       正常播放
.pdf       浏览器原生预览
.html      sandbox iframe
.svg       img方式显示
unknown    Unsupported + Download
```

不能破坏原文件下载。

---

# 38. Phase 2 — 文本类 Preview

增加：

```text
Text
Code
Markdown
JSON
XML
```

增加依赖：

```text
pulldown-cmark
ammonia
```

加入：

```text
4 MiB限制
UTF-8检查
Markdown sanitize
JSON fallback
全局Semaphore
```

### Phase 2 验收

```text
正常Markdown正常显示

<script>
不能执行

非法JSON
→ Text

5MiB Text
→ too_large

非UTF-8
→ unsupported_encoding

高并发Markdown
→ 不超过Semaphore限制
```

---

# 39. Phase 3 — CSV / Archive

增加：

```text
CSV
TSV
ZIP
TAR
TAR.GZ
```

增加：

```text
csv
zip
tar
flate2
```

加入：

```text
CSV资源限制
Archive entry限制
Archive总未压缩大小限制
Compression ratio检测
```

### Phase 3 验收

```text
CSV 500行以内正常显示

超出限制
→ 截断/提示

10,000+ Archive entries
→ resource_limit

异常压缩比ZIP
→ 拒绝预览

损坏ZIP
→ Download仍正常
```

---

# 40. Phase 4 — 大文件和 PreviewRuntime

增加：

```text
Windowed Text Preview
Opaque Cursor
PreviewRuntime
分级Semaphore
Cancellation
Singleflight
```

### Phase 4 验收

例如：

```text
2GB server.log
```

不得整体读取。

连续快速切换文件：

```text
旧任务停止
```

大量同一 Heavy Preview：

```text
Singleflight只生成一次
```

---

# 41. Phase 5 — Thumbnail + PreviewCache

加入：

```text
Video poster
PDF thumbnail
特殊图片转换
PreviewSignature
PreviewCache
LRU
Cache quota
```

必须确保：

```text
raw_url每次现签
raw_url不进入Cache
权限每次重新检查
```

---

# 42. Phase 6 — Magika

最后加入：

```text
Magika
TypeCache
```

只针对：

```text
无扩展名
Unknown
.bin
.dat
```

不得影响：

```text
.jpg
.pdf
.md
.mp4
```

的快速路径。

---

# 43. 后端测试清单

必须覆盖：

```text
Extension → PreviewType

PreviewType → PreviewStrategy

权限验证

用户base_path限制

Path Traversal

Symlink escape

UTF-8检查

4 MiB文本限制

Markdown XSS

HTML sandbox

SVG非inline

JSON fallback

CSV size/row/column限制

Archive entry限制

Archive total uncompressed限制

Archive compression ratio

Broken archive

Semaphore并发限制

Cancellation

Singleflight

PreviewSignature失效

Cache LRU

raw_url不缓存

raw_url重新签名

Cache Hit仍然鉴权

Magika Unknown fallback
```

---

# 44. 测试 Fixtures

建议：

```text
tests/fixtures/preview/
├── image.jpg
├── image.svg
├── video.mp4
├── audio.mp3
├── document.pdf
│
├── readme.md
├── malicious.md
├── large.txt
├── code.rs
│
├── valid.json
├── invalid.json
├── sample.xml
├── sample.csv
│
├── sample.zip
├── broken.zip
├── zip-bomb-like.zip
│
├── malicious.html
├── malicious.svg
│
├── unknown.bin
└── no-extension
```

另动态创建：

```text
symlink fixtures
```

用于 symlink escape 测试。

---

# 45. 性能底线

必须严格遵守：

```text
目录列表
≠
内容识别
```

目录打开时只使用：

```text
name
extension
size
mtime
已有metadata
```

不能执行：

```text
读取文件内容
MIME深度检测
Magika
Markdown解析
Archive解析
```

只有：

```text
用户真正打开文件
```

才进入 Preview 系统。

---

# 46. 明确不做

第一阶段及基础版本不做：

```text
Monaco Editor
PDF.js
Office在线编辑
服务器视频转码
所有文件Magika
目录批量MIME扫描
自动解压Archive
Markdown直接innerHTML
SVG inline
完整插件系统
数百格式支持
```

---

# 47. 最终代码改动范围

后端：

```text
src/
├── preview/
│   ├── mod.rs
│   ├── types.rs
│   ├── detector.rs
│   ├── resolver.rs
│   ├── limits.rs
│   └── processor/
│
├── server/
│   ├── preview.rs
│   ├── mod.rs
│   ├── fs.rs
│   └── stream.rs
```

其中：

```text
stream.rs
```

Phase 1 原则上不改，继续使用现有 Raw + Range 实现。

前端：

```text
web/src/pages/home/file/
└── File.tsx

web/src/components/preview/
├── FilePreviewLayout.tsx
├── PreviewHeader.tsx
├── PreviewLoading.tsx
├── PreviewError.tsx
├── UnsupportedPreview.tsx
├── ImagePreview.tsx
├── VideoPreview.tsx
├── AudioPreview.tsx
├── PdfPreview.tsx
├── HtmlPreview.tsx
├── MarkdownPreview.tsx
├── TextPreview.tsx
├── CodePreview.tsx
├── JsonPreview.tsx
├── XmlPreview.tsx
├── CsvPreview.tsx
└── ArchivePreview.tsx
```

---

# 48. 最终开发顺序

```text
Phase 1
基础架构
+ Direct Preview
+ 权限边界

        ↓

Phase 2
Text / Code / Markdown / JSON / XML
+ UTF-8
+ 4MiB限制
+ Sanitizer
+ Semaphore

        ↓

Phase 3
CSV / TSV / Archive
+ CSV资源限制
+ Archive安全限制
+ ZIP bomb防护

        ↓

Phase 4
Windowed Preview
+ Opaque Cursor
+ Cancellation
+ Worker分类
+ Singleflight

        ↓

Phase 5
Thumbnail
+ PreviewSignature
+ PreviewCache
+ LRU
+ 1GiB quota
+ raw_url现签

        ↓

Phase 6
Magika
+ TypeCache
+ Unknown智能识别
```

---

# 49. 不可违反的设计规则

整个实现过程中以下规则视为硬约束：

1. 浏览器能原生预览的文件不进入重型 Preview Job。
2. Preview 只通过 `StorageManager` 访问文件。
3. Preview Cache 命中不能绕过权限检查。
4. Signed `raw_url` 不得进入长期缓存。
5. Signed `raw_url` 每次 Preview 响应重新生成。
6. Phase 2 起所有 Processed Preview 必须受 Semaphore 限制。
7. 大文件不能无限读入内存。
8. Archive 不默认解压。
9. Archive 必须限制 entry、累计大小和异常压缩比。
10. HTML iframe 不允许 `allow-same-origin`。
11. SVG 不允许 inline。
12. Markdown HTML 必须 sanitize。
13. Symlink 安全规则不得被 Preview 绕过。
14. Magika 不参与普通目录扫描。
15. Preview 失败必须保留 Raw / Download。
16. Preview 功能不得成为访问文件的必需条件。

---

# 50. 最终目标

最终 Rulist Preview 应达到：

```text
扩展名
负责快速识别

Magika
负责未知类型兜底

Browser
负责图片、音视频、PDF等原生预览

Rust Processor
负责Markdown、文本、结构化文件

Semaphore
负责轻量任务资源保护

PreviewRuntime
负责重型任务

PreviewCache
负责避免重复转换

Singleflight
负责避免同一任务重复执行

SolidJS
负责统一UI

StorageManager
负责统一数据源

现有Signed Raw + Range
负责原文件安全访问
```

最终原则可以概括为：

> **能直接展示就直接展示，需要解析才解析，需要转换才转换，需要 AI 才调用 AI；所有预览都必须有资源边界、安全边界和可靠降级。**