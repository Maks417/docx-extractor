#!/usr/bin/env node
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { readFile } from "node:fs/promises";
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
  "and base64-encoded images. See https://github.com/Maks417/docx-extractor for schema.",
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
    };
    if (typeof args.path !== "string" || args.path.length === 0) {
      return {
        isError: true,
        content: [{ type: "text", text: "Argument 'path' must be a non-empty string." }],
      };
    }
    const pretty = args.pretty === true;

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

    const cliArgs = pretty ? ["--pretty", args.path] : [args.path];
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
    return { content: [{ type: "text", text: result.stdout }] };
  });

  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  process.stderr.write(`docx-extractor-mcp fatal: ${(err as Error).stack ?? err}\n`);
  process.exit(1);
});
