# Using docx-extractor in a Claude skill

`docx-extractor` is a single static binary that turns a `.docx` file into structured JSON. This guide is for skill authors who want their skill to read Word documents without depending on Microsoft Office, `python-docx`, `mammoth`, or anything else.

## Why a binary

Word documents are ZIP archives of XML, and parsing them well — preserving paragraphs, tables, footnotes, comments, headers/footers, tracked changes, and image positions — needs a non-trivial amount of code. Shipping that as a skill dependency in every model invocation wastes context. A binary that emits JSON keeps the skill prompt tiny.

## Wire-up

In your skill's working directory, fetch the binary that matches the user's platform (one-time setup), then invoke it from skill steps:

```bash
# Linux example
curl -L https://github.com/Maks417/docx-extractor/releases/latest/download/docx-extractor-linux-x86_64 \
  -o ./bin/docx-extractor
chmod +x ./bin/docx-extractor

# Then in skill steps:
./bin/docx-extractor input.docx > document.json
```

Release assets:

| Platform | Asset |
|---|---|
| Linux x86-64 | `docx-extractor-linux-x86_64` |
| macOS Intel | `docx-extractor-macos-x86_64` |
| macOS Apple Silicon | `docx-extractor-macos-aarch64` |
| Windows x86-64 | `docx-extractor-windows-x86_64.exe` |

Exit code is `0` on success, `1` on any error (invalid path, not a DOCX, malformed XML). Error message goes to stderr.

## JSON contract

```jsonc
{
  "source": "report.docx",         // filename only

  "metadata": {                    // omitted entirely if no core.xml
    "title":            "...",
    "author":           "...",
    "last_modified_by": "...",
    "created":          "2024-01-15T10:30:00Z",
    "modified":         "..."
  },

  "sections": [                    // ordered body content
    { "type": "heading",   "level": 1, "text": "...",
      "images": ["image1.png"], "footnote_refs": [1], "endnote_refs": [2] },
    { "type": "paragraph", "text": "..." },
    { "type": "list_item", "level": 0, "text": "..." },
    { "type": "table",     "rows": [[{ "text": "...", "images": [] }]] }
  ],

  "headers":   [{ "type": "default" | "first" | "even", "sections": [/* Section[] */] }],
  "footers":   [{ "type": "default" | "first" | "even", "sections": [/* Section[] */] }],
  "footnotes": [{ "id": 1, "sections": [/* Section[] */] }],
  "endnotes":  [{ "id": 1, "sections": [/* Section[] */] }],

  "comments": [
    { "id": 0, "author": "Jane", "date": "2024-02-01T12:00:00Z",
      "anchor": { "section_index": 4, "char_start": 12, "char_end": 27 },
      "sections": [/* Section[] */] }
  ],

  "revisions": [
    { "kind": "insert" | "delete",
      "author": "Jane", "date": "...",
      "anchor": { "section_index": 4, "char_start": 0, "char_end": 8 },
      "text": "the added or removed text" }
  ],

  "images": [
    { "id": "image1.png", "mime_type": "image/png", "base64": "..." }
  ]
}
```

### Fields that may be absent

Any of the optional top-level arrays (`headers`, `footers`, `footnotes`, `endnotes`, `comments`, `revisions`) and per-section refs (`images`, `footnote_refs`, `endnote_refs`) are omitted when empty rather than emitted as `[]`. Check with `field in obj` / `.get('field', [])` style access. `metadata`, `metadata.title`, etc. are omitted when absent too.

### Anchors

Comments and revisions reference body content by `{section_index, char_start, char_end}`:

- `section_index` is an index into the top-level `sections[]` array
- `char_start` / `char_end` are byte offsets into that section's `text` field (after trimming)
- For a deletion, `char_start == char_end` — the deleted text was never in the body; find it in `revisions[].text`
- A comment that spans multiple paragraphs anchors to the section where it started, with `char_end` at the end of that section's text

### Hyperlinks

Hyperlinks are inlined directly into section text as markdown: `[link text](https://example.com)`. There is no separate `hyperlinks` array.

## Patterns

### "Summarize this document"

```python
import json, subprocess
doc = json.loads(subprocess.check_output(["./bin/docx-extractor", path]))

title = doc.get("metadata", {}).get("title", doc["source"])
body  = "\n\n".join(
    section_to_text(s) for s in doc["sections"]
)

def section_to_text(s):
    if s["type"] == "heading":   return "#" * s["level"] + " " + s["text"]
    if s["type"] == "list_item": return "  " * s["level"] + "- " + s["text"]
    if s["type"] == "paragraph": return s["text"]
    if s["type"] == "table":     return "\n".join(" | ".join(c["text"] for c in row) for row in s["rows"])
```

### "Find Figure 2"

`doc["images"]` lists every image. To find the *caption*, look at sections where `images` is non-empty:

```python
captions = [s for s in doc["sections"]
            if s.get("images") and "figure 2" in s.get("text", "").lower()]
```

### "Summarize the review comments"

```python
for c in doc.get("comments", []):
    body  = " ".join(s["text"] for s in c["sections"] if s["type"] == "paragraph")
    where = doc["sections"][c["anchor"]["section_index"]]
    quoted = where.get("text", "")[c["anchor"]["char_start"]:c["anchor"]["char_end"]]
    print(f"{c['author']} on \"{quoted}\": {body}")
```

### "What was changed?"

```python
for r in doc.get("revisions", []):
    verb = "inserted" if r["kind"] == "insert" else "deleted"
    print(f"{r['author']} {verb}: {r['text']!r}")
```

### Reading footnote text from a reference

```python
notes = {n["id"]: n for n in doc.get("footnotes", [])}
for s in doc["sections"]:
    for ref_id in s.get("footnote_refs", []):
        text = " ".join(p["text"] for p in notes[ref_id]["sections"]
                        if p["type"] == "paragraph")
        print(f"[^{ref_id}]: {text}")
```

### Getting a base64 image as bytes

```python
import base64
img = next(i for i in doc["images"] if i["id"] == "image1.png")
bytes_ = base64.b64decode(img["base64"])
```

## Limitations to communicate to users

These are documented in [CLAUDE.md](CLAUDE.md) but worth surfacing in skill help text:

- **No inline styling**: bold/italic/color/font are lost. The skill cannot tell whether a span was emphasized.
- **No equations**: `<m:oMath>` content is not extracted.
- **No SmartArt / shapes**: only raster images come through.
- **Localized headings**: non-English heading styles only appear as headings if they set `outlineLvl`; otherwise they look like paragraphs.
- **Nested-table structure is flattened** into the outer cell's text — the inner row/column shape is lost.
- **No custom doc properties**: `docProps/custom.xml` is not parsed.

## Performance notes

- Single-pass: ~1 ms per page for typical documents.
- Memory: peaks at ~2x the document size during base64 encoding of images.
- The binary is small (~2 MB stripped) and has zero runtime dependencies.
