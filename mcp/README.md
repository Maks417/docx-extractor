# docx-extractor-mcp

[Model Context Protocol](https://modelcontextprotocol.io) server that wraps the [`docx-extractor`](https://github.com/Maks417/docx-extractor) Rust CLI. Lets Claude Desktop (or any MCP-aware client) read `.docx` files and return structured JSON — paragraphs, headings, lists, tables, footnotes, comments, tracked changes, and base64-encoded images — without Microsoft Office.

On first call the server downloads the matching `docx-extractor` binary from GitHub Releases into `~/.cache/docx-extractor-mcp/<version>/`, verifies its SHA-256, and caches it for subsequent runs.

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

Returns the JSON document as text. See the [main project README](https://github.com/Maks417/docx-extractor#output) for the full schema.

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
