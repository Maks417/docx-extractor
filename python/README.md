# docx-extractor-cli

Python wrapper around the [`docx-extractor`](https://github.com/Maks417/docx-extractor) Rust CLI. Extracts text, headings, lists, tables, footnotes, comments (with anchors), tracked changes, headers/footers, and embedded images from a `.docx` file and returns structured JSON.

The wheel **bundles** the prebuilt Rust binary for your platform — no Rust toolchain needed, no network access required at install time. This makes it usable inside restricted-egress environments like Claude Desktop's analysis sandbox where downloading directly from GitHub Releases is blocked.

> The PyPI **distribution name** is `docx-extractor-cli` (the unhyphenated `docx-extractor` is taken on PyPI by an unrelated project). The **Python import name** is `docx_extractor`, and the **console script** is `docx-extractor`.

## Install

```bash
pip install docx-extractor-cli
```

Prebuilt wheels are published for:

- Linux x86-64 (manylinux 2.28+ — Debian 11+, RHEL 8+, Ubuntu 20.04+)
- macOS x86-64 (Intel)
- macOS aarch64 (Apple Silicon)
- Windows x86-64

For other targets, [build the Rust binary from source](https://github.com/Maks417/docx-extractor).

## Use from Python

```python
import docx_extractor

doc = docx_extractor.extract("/path/to/file.docx")
print(doc["metadata"]["title"])
for section in doc["sections"]:
    print(section)
```

`extract()` signature:

```python
docx_extractor.extract(
    path: str,
    *,
    pretty: bool = False,
    output: str | None = None,
    no_images: bool = False,
    max_image_bytes: int | None = None,
    timeout: float | None = None,
) -> dict | None
```

- Returns the parsed JSON document as a `dict` (or `None` when `output` is given — the JSON is written to that path instead).
- Raises `docx_extractor.DocxExtractorError` on non-zero exit, carrying the binary's stderr text.

## Use from the shell

The wheel installs a `docx-extractor` console script that's a thin pass-through to the bundled binary. Same CLI as the Rust release:

```bash
docx-extractor /path/to/file.docx --pretty
docx-extractor /path/to/file.docx --output document.json --no-images
docx-extractor /path/to/file.docx --max-image-bytes 5242880
```

Flags:

| Flag | Description |
|---|---|
| `--pretty` / `-p` | Pretty-print JSON. |
| `--output <path>` / `-o <path>` | Write to a file instead of stdout. |
| `--no-images` | Skip base64 image bytes (per-section `images` references are preserved). |
| `--max-image-bytes <n>` | Per-image size cap (default: 10 MiB). |

## Use inside Claude Desktop's analysis sandbox

This is the primary motivation for the wheel. When a user uploads a `.docx` to a Claude Desktop chat, the file lands at `/mnt/user-data/uploads/...` inside a Linux sandbox where GitHub Release downloads are blocked but PyPI is allowlisted.

```bash
pip install docx-extractor
docx-extractor /mnt/user-data/uploads/foo.docx --no-images --output /tmp/doc.json
```

Then parse `/tmp/doc.json` in Python. `--no-images` is strongly recommended for chat workflows — base64 image bytes dominate token cost. Opt back in only when the user explicitly asks about images.

## JSON schema

See the [main project README](https://github.com/Maks417/docx-extractor#output) for the full schema.

## macOS Gatekeeper

Unsigned binaries delivered via `pip install` run fine as subprocess invocations (no GUI launch, no quarantine prompt). If you ever hit a Gatekeeper warning when invoking the binary directly:

```bash
xattr -dr com.apple.quarantine "$(python -c 'import docx_extractor._binary as b; print(b.path())')"
```

## Versioning

The PyPI package version mirrors the Rust binary version exactly. Installing `docx-extractor-cli==0.4.0` ships the `v0.4.0` Rust binary.

## License

MIT — see the [main repo](https://github.com/Maks417/docx-extractor).
