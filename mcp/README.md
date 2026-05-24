# docx-extractor-mcp

[Model Context Protocol](https://modelcontextprotocol.io) server that wraps the [`docx-extractor`](https://github.com/Maks417/docx-extractor) Rust CLI. Lets Claude Desktop (or any MCP-aware client) read `.docx` files and return structured JSON — paragraphs, headings, lists, tables, footnotes, comments, tracked changes, and base64-encoded images — without Microsoft Office.

On first call the server downloads the matching `docx-extractor` binary from GitHub Releases into `~/.cache/docx-extractor-mcp/<version>/`, verifies its SHA-256, and caches it for subsequent runs.

> **Working with files uploaded into Claude Desktop?** The MCP server runs on the host machine and cannot see files inside Claude Desktop's analysis sandbox (`/mnt/user-data/uploads/...`). For that case, install the Python wrapper directly inside the sandbox instead: `pip install docx-extractor-cli` — it bundles the same binary and works on the sandbox upload path. See the [Python package README](https://github.com/Maks417/docx-extractor/tree/main/python).

## Install in Claude Desktop

Add this to your `claude_desktop_config.json` (Settings → Developer → Edit Config):

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

Restart Claude Desktop. The `extract_docx` tool will be available — drop a `.docx` file into a chat and ask Claude to read it.

## Install in Claude Code

```bash
claude mcp add docx-extractor -- npx -y docx-extractor-mcp
```

## Tool surface

### `extract_docx`

Extract structured JSON from a `.docx` file.

| Argument | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | Absolute path to the `.docx` file. |
| `pretty` | boolean | no | Pretty-print the JSON output. |
| `outputPath` | string | no | If set, write JSON to this absolute path and return a short summary instead of the full document. Use this for large docs to keep them out of the conversation. |
| `includeImages` | boolean | no | Include base64-encoded image bytes in the response (default: `true`). Set `false` to skip the top-level `images` array; per-section image references are still preserved. |
| `maxImageBytes` | integer | no | Per-image size cap in bytes (default: `10485760` / 10 MB). Larger images are skipped with a warning. |

Returns the JSON document as text (or, when `outputPath` is set, a one-line summary). See the [main project README](https://github.com/Maks417/docx-extractor#output) for the full schema.

### Verification & offline mode

On first call the installer downloads `SHA256SUMS.txt` from the release and verifies the binary against it. If the checksum file is unreachable (offline, mirror, private fork), set `DOCX_EXTRACTOR_MCP_SKIP_CHECKSUM=1` in the MCP server's environment to bypass verification — only use this when you trust the upstream.

## Supported platforms

Prebuilt binaries are published for:

- Linux x86-64
- macOS x86-64 (Intel)
- macOS aarch64 (Apple Silicon)
- Windows x86-64

Other targets (Linux aarch64, Windows aarch64, etc.) need to be built from source — clone the [main repo](https://github.com/Maks417/docx-extractor) and `cargo build --release`.

## Versioning

The npm package version mirrors the Rust binary version. Installing `docx-extractor-mcp@0.2.2` always downloads the `v0.2.2` Rust binary — no surprise upgrades.

## License

MIT — see the [main repo](https://github.com/Maks417/docx-extractor).
