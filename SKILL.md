---
name: docx-extractor
description: >-
  Extract and analyze Word .docx files via the docx-extractor CLI (JSON with
  sections, tables, comments, footnotes, tracked changes, images). Use when the
  user provides a .docx file or asks to read, summarize, or analyze a Word
  document. Do not use for creating or editing docx, or for PDF, PPTX, or XLSX.
---

# docx-extractor

You have access to `docx-extractor` — a fast static binary that converts any `.docx` Word file into structured JSON. Use it every time the user wants to read or analyze a Word document. Never attempt to parse `.docx` files manually or with Python libraries.

> On Claude Desktop (which has no shell), use the [`docx-extractor-mcp`](https://github.com/Maks417/docx-extractor/tree/master/mcp) MCP server instead — it handles binary download for you. The shell-based setup below is for Claude Code.

## Setup (one-time per machine)

Before the first use, detect the OS and download the correct binary from the latest GitHub release:

```bash
# Detect OS and architecture
OS=$(uname -s 2>/dev/null)
ARCH=$(uname -m 2>/dev/null)
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"

if [[ "$OS" == "Linux" ]]; then
  ASSET="docx-extractor-linux-x86_64"
elif [[ "$OS" == "Darwin" && "$ARCH" == "arm64" ]]; then
  ASSET="docx-extractor-macos-aarch64"
elif [[ "$OS" == "Darwin" ]]; then
  ASSET="docx-extractor-macos-x86_64"
fi

curl -fsSL "https://github.com/Maks417/docx-extractor/releases/latest/download/$ASSET" \
  -o "$BIN_DIR/docx-extractor"
chmod +x "$BIN_DIR/docx-extractor"
```

For Windows (PowerShell):

```powershell
$dir = "$env:USERPROFILE\.local\bin"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Invoke-WebRequest `
  -Uri "https://github.com/Maks417/docx-extractor/releases/latest/download/docx-extractor-windows-x86_64.exe" `
  -OutFile "$dir\docx-extractor.exe"
```

Check before downloading — skip setup if already installed:

```bash
command -v docx-extractor 2>/dev/null || ~/.local/bin/docx-extractor --version 2>/dev/null
```

## Extracting a document

```bash
~/.local/bin/docx-extractor path/to/file.docx > document.json
# pretty-print for debugging:
~/.local/bin/docx-extractor path/to/file.docx --pretty
```

Exit code `0` = success. Exit code `1` = error (details on stderr). On Windows use `docx-extractor.exe`.

## JSON output shape

```jsonc
{
  "source": "report.docx",
  "metadata": { "title": "...", "author": "...", "created": "...", "modified": "..." },
  "sections": [
    { "type": "heading",   "level": 1, "text": "Introduction" },
    { "type": "paragraph", "text": "Body text.", "footnote_refs": [1], "images": ["img1.png"] },
    { "type": "list_item", "level": 0, "text": "First item" },
    { "type": "table",     "rows": [[{ "text": "Cell A" }, { "text": "Cell B" }]] }
  ],
  "headers":   [{ "type": "default", "sections": [ /* Section[] */ ] }],
  "footers":   [{ "type": "default", "sections": [ /* Section[] */ ] }],
  "footnotes": [{ "id": 1, "sections": [ /* Section[] */ ] }],
  "endnotes":  [{ "id": 1, "sections": [ /* Section[] */ ] }],
  "comments":  [{ "id": 0, "author": "Jane", "date": "...",
                  "anchor": { "section_index": 4, "char_start": 12, "char_end": 27 },
                  "sections": [ /* Section[] */ ] }],
  "revisions": [{ "kind": "insert", "author": "...", "date": "...",
                  "anchor": { "section_index": 4, "char_start": 0, "char_end": 8 },
                  "text": "added or removed text" }],
  "images":    [{ "id": "img1.png", "mime_type": "image/png", "base64": "..." }]
}
```

All optional arrays (`headers`, `footers`, `footnotes`, `endnotes`, `comments`, `revisions`, `images`) and per-section fields (`images`, `footnote_refs`, `endnote_refs`) are **omitted when empty** — always use `.get("field", [])` / `field in obj` style access.

Hyperlinks are inlined as markdown `[text](url)` directly in section text.

## Common task patterns

### Summarize document body

```python
import json, subprocess

doc = json.loads(subprocess.check_output(["docx-extractor", "file.docx"]))
title = doc.get("metadata", {}).get("title", doc["source"])

def section_to_text(s):
    if s["type"] == "heading":   return "#" * s["level"] + " " + s["text"]
    if s["type"] == "list_item": return "  " * s["level"] + "- " + s["text"]
    if s["type"] == "table":     return "\n".join(" | ".join(c["text"] for c in r) for r in s["rows"])
    return s.get("text", "")

body = "\n\n".join(section_to_text(s) for s in doc["sections"])
```

### List review comments with quoted context

```python
for c in doc.get("comments", []):
    text  = " ".join(s["text"] for s in c["sections"] if s["type"] == "paragraph")
    ctx   = doc["sections"][c["anchor"]["section_index"]]
    quote = ctx.get("text", "")[c["anchor"]["char_start"]:c["anchor"]["char_end"]]
    print(f'{c["author"]} on "{quote}": {text}')
```

### Show tracked changes

```python
for r in doc.get("revisions", []):
    verb = "inserted" if r["kind"] == "insert" else "deleted"
    print(f'{r["author"]} {verb}: {r["text"]!r}')
```

### Read footnotes from inline references

```python
notes = {n["id"]: n for n in doc.get("footnotes", [])}
for s in doc["sections"]:
    for ref in s.get("footnote_refs", []):
        note_text = " ".join(p["text"] for p in notes[ref]["sections"] if p["type"] == "paragraph")
        print(f"[^{ref}]: {note_text}")
```

### Extract images as files

```python
import base64
for img in doc.get("images", []):
    with open(img["id"], "wb") as f:
        f.write(base64.b64decode(img["base64"]))
```

## Known limitations

- **No inline styling**: bold, italic, color, font size are not captured.
- **No equations**: `<m:oMath>` content is skipped.
- **No SmartArt or shapes**: only raster images (PNG, JPEG, GIF, BMP, TIFF, WebP) are extracted; WMF/EMF are skipped.
- **Localized heading styles**: non-English style names (e.g. `Titre1`) resolve to headings only if they also set `<w:outlineLvl>`; otherwise they appear as plain paragraphs.
- **Nested tables are flattened**: inner table content is preserved as text inside the outer cell; inner row/column structure is lost.
- **10 MB image cap**: images larger than 10 MB are skipped with a stderr warning.
- **No custom document properties**: only `docProps/core.xml` fields are parsed.
