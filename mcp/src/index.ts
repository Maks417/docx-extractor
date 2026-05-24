#!/usr/bin/env node
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { readFile, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { ensureBinary } from "./installer.js";
import { runBinary } from "./runner.js";

async function readPackageVersion(): Promise<string> {
  const here = dirname(fileURLToPath(import.meta.url));
  const raw = await readFile(join(here, "..", "package.json"), "utf8");
  return (JSON.parse(raw) as { version: string }).version;
}

const TOOL_DESCRIPTION = [
  "Extract structured JSON from a Microsoft Word .docx file.",
  "Returns paragraphs, headings, lists, tables, footnotes, endnotes, headers/footers,",
  "comments (with anchors), tracked changes (insertions/deletions with author + date),",
  "and base64-encoded images. Prefer this tool over Python .docx libraries for fidelity",
  "on tracked changes, comment anchors, and embedded images.",
  "For large documents, set outputPath to write JSON to a file (avoids loading megabytes",
  "into the conversation) and/or set includeImages=false to skip base64 image bytes.",
  "See https://github.com/Maks417/docx-extractor for schema.",
].join(" ");

async function main(): Promise<void> {
  const version = await readPackageVersion();
  const server = new Server(
    { name: "docx-extractor", version },
    { capabilities: { tools: {} } },
  );

  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: [
      {
        name: "extract_docx",
        description: TOOL_DESCRIPTION,
        inputSchema: {
          type: "object",
          properties: {
            path: {
              type: "string",
              description: "Absolute path to the .docx file to extract.",
            },
            pretty: {
              type: "boolean",
              description: "Pretty-print the returned JSON (default: false).",
            },
            outputPath: {
              type: "string",
              description:
                "If set, write the JSON to this absolute path and return a short summary (file size, section count) instead of the full document. Use this for large docs to keep them out of the conversation.",
            },
            includeImages: {
              type: "boolean",
              description:
                "Include base64-encoded image bytes in the response (default: true). Set false to skip the top-level `images` array; per-section image references are still preserved.",
            },
            maxImageBytes: {
              type: "integer",
              description:
                "Per-image size cap in bytes. Images larger than this are skipped. Default: 10485760 (10 MB).",
              minimum: 1,
            },
          },
          required: ["path"],
        },
      },
    ],
  }));

  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    if (req.params.name !== "extract_docx") {
      return {
        isError: true,
        content: [{ type: "text", text: `Unknown tool: ${req.params.name}` }],
      };
    }
    const args = (req.params.arguments ?? {}) as {
      path?: unknown;
      pretty?: unknown;
      outputPath?: unknown;
      includeImages?: unknown;
      maxImageBytes?: unknown;
    };
    if (typeof args.path !== "string" || args.path.length === 0) {
      return {
        isError: true,
        content: [{ type: "text", text: "Argument 'path' must be a non-empty string." }],
      };
    }
    const pretty = args.pretty === true;
    const outputPath =
      typeof args.outputPath === "string" && args.outputPath.length > 0
        ? args.outputPath
        : null;
    const includeImages = args.includeImages !== false; // default true
    const maxImageBytes =
      typeof args.maxImageBytes === "number" && Number.isFinite(args.maxImageBytes)
        ? Math.max(1, Math.floor(args.maxImageBytes))
        : null;

    let binPath: string;
    try {
      binPath = await ensureBinary();
    } catch (err) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: `Failed to install docx-extractor binary: ${(err as Error).message}`,
          },
        ],
      };
    }

    const cliArgs: string[] = [];
    if (pretty) cliArgs.push("--pretty");
    if (outputPath) cliArgs.push("--output", outputPath);
    if (!includeImages) cliArgs.push("--no-images");
    if (maxImageBytes !== null) cliArgs.push("--max-image-bytes", String(maxImageBytes));
    cliArgs.push(args.path);

    const result = await runBinary(binPath, cliArgs);
    if (result.code !== 0) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: result.stderr.trim() || `docx-extractor exited with code ${result.code}`,
          },
        ],
      };
    }

    if (outputPath) {
      let sizeBytes = 0;
      try {
        sizeBytes = (await stat(outputPath)).size;
      } catch {
        // Fall through with size = 0; the file was written successfully (exit 0).
      }
      const warning = result.stderr.trim();
      const summary = `Wrote ${sizeBytes} bytes to ${outputPath}.`;
      return {
        content: [
          {
            type: "text",
            text: warning ? `${summary}\n\n${warning}` : summary,
          },
        ],
      };
    }
    return { content: [{ type: "text", text: result.stdout }] };
  });

  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  process.stderr.write(`docx-extractor-mcp fatal: ${(err as Error).stack ?? err}\n`);
  process.exit(1);
});
