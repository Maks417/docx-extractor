# docx-extractor

[![Build](https://github.com/Maks417/docx-extractor/actions/workflows/build.yml/badge.svg)](https://github.com/Maks417/docx-extractor/actions/workflows/build.yml)
[![Release](https://github.com/Maks417/docx-extractor/actions/workflows/release.yml/badge.svg)](https://github.com/Maks417/docx-extractor/actions/workflows/release.yml)
[![CodeQL](https://github.com/Maks417/docx-extractor/actions/workflows/github-code-scanning/codeql/badge.svg)](https://github.com/Maks417/docx-extractor/actions/workflows/github-code-scanning/codeql)

A small Rust CLI that converts a `.docx` file into structured JSON — paragraphs, headings, lists, tables, footnotes, headers/footers, comments, tracked changes, and base64-encoded images. Designed to be consumed programmatically (e.g. by a [Claude skill](SKILL.md)) without needing Microsoft Office.

## Use from Claude Desktop (MCP)

Easiest way for end users. Add this to your `claude_desktop_config.json` (Settings → Developer → Edit Config), restart Claude Desktop, and the `extract_docx` tool is available:

```jsonc
{
  "mcpServers": {
    "docx-extractor": {
      "command": "npx",
      "args": ["-y", "docx-extractor-mcp"]
    }
  }
}
```

The MCP wrapper auto-downloads the matching platform binary on first call and caches it in `~/.cache/docx-extractor-mcp/<version>/`. Requires Node.js 18+. See [mcp/README.md](mcp/README.md) for details.

## Install

Download the latest binary for your platform from the [Releases](../../releases) page:

| Platform | Asset |
|---|---|
| Linux x86-64 | `docx-extractor-linux-x86_64` |
| macOS Intel | `docx-extractor-macos-x86_64` |
| macOS Apple Silicon | `docx-extractor-macos-aarch64` |
| Windows x86-64 | `docx-extractor-windows-x86_64.exe` |

Make it executable (`chmod +x docx-extractor-linux-x86_64`) and put it on your `PATH`, or invoke it by full path. The Windows asset is named `docx-extractor-windows-x86_64.exe` for clarity on the Releases page — rename it to `docx-extractor.exe` once it's on your `PATH`.

Or build from source:

```bash
cargo build --release
# binary at target/release/docx-extractor
```

## Usage

```bash
docx-extractor path/to/file.docx                       # JSON to stdout
docx-extractor path/to/file.docx --pretty              # pretty-printed
docx-extractor path/to/file.docx --output out.json     # write to file
docx-extractor path/to/file.docx --no-images           # skip base64 image bytes
docx-extractor path/to/file.docx --max-image-bytes 1048576   # cap individual images at 1 MB
```

## Output

```jsonc
{
  "source": "report.docx",
  "metadata": { "title": "...", "author": "...", "created": "..." },
  "sections": [
    { "type": "heading",   "level": 1, "text": "Introduction" },
    { "type": "paragraph", "text": "See [^1].", "footnote_refs": [1] },
    { "type": "list_item", "level": 0, "text": "First bullet" },
    { "type": "paragraph", "text": "Figure caption", "images": ["image1.png"] },
    { "type": "table", "rows": [[{ "text": "A" }, { "text": "B" }]] }
  ],
  "headers":   [{ "type": "default", "sections": [/* ... */] }],
  "footers":   [{ "type": "default", "sections": [/* ... */] }],
  "footnotes": [{ "id": 1, "sections": [{ "type": "paragraph", "text": "..." }] }],
  "comments":  [{ "id": 0, "author": "Jane", "anchor": { "section_index": 2, "char_start": 4, "char_end": 15 },
                  "sections": [{ "type": "paragraph", "text": "Looks good." }] }],
  "revisions": [{ "kind": "delete", "author": "Jane", "text": "removed words",
                  "anchor": { "section_index": 1, "char_start": 5, "char_end": 5 } }],
  "images":    [{ "id": "image1.png", "mime_type": "image/png", "base64": "..." }]
}
```

Empty arrays and null fields are omitted. See [CLAUDE.md](CLAUDE.md) for the full schema and known limitations.

## What is extracted

- Paragraphs, headings (`Heading1..9` or `outlineLvl`), list items with `level`
- Tables (cells are `{text, images}`; nested tables are flattened into the outer cell as joined text)
- Tracked insertions and deletions — author, date, anchor (deletions are removed from body text but surfaced in `revisions[]`)
- Comments — author, date, body, and anchor into the body section it wraps
- Footnotes and endnotes — full nested sections + inline `footnote_refs` / `endnote_refs`
- Headers and footers — full nested sections, typed `default` / `first` / `even`
- Hyperlinks — inlined as markdown `[text](url)`
- Images — PNG / JPEG / GIF / BMP / TIFF / WebP, base64-encoded; per-section `images[filename]` ties them to their location
- Core metadata — title, author, last-modified-by, created, modified

## License

MIT
