#!/usr/bin/env node
// Kaleidoscope MCP server. Exposes the rendering engine as a single tool so an
// AI agent can turn HTML and CSS into an image and see what it produced, with
// no headless browser. It shells out to the `kaleidoscope` binary, which boots
// in milliseconds and needs a couple of megabytes of memory, so an agent can
// close the visual feedback loop without spinning up Chromium.

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { execFile } from "node:child_process";

const BIN = process.env.KALEIDOSCOPE_BIN || "kaleidoscope";
const MAX_DIM = 4096;

function clampDim(value, fallback) {
  const n = Number.isFinite(value) ? Math.floor(value) : fallback;
  return Math.max(1, Math.min(MAX_DIM, n));
}

function snapshot(html, css, width, height) {
  return new Promise((resolve, reject) => {
    execFile(
      BIN,
      ["snapshot", "--html", html, "--css", css, "--width", String(width), "--height", String(height)],
      { timeout: 20000, maxBuffer: 64 * 1024 * 1024 },
      (err, stdout, stderr) => {
        if (err) {
          reject(new Error((stderr || err.message || "render failed").trim()));
        } else {
          resolve(stdout.trim());
        }
      }
    );
  });
}

const server = new Server(
  { name: "kaleidoscope", version: "0.1.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "render_html",
      description:
        "Render an HTML string with an optional CSS string to a PNG image using the Kaleidoscope engine, and return the image. Use this to see how a snippet of HTML and CSS lays out without a browser.",
      inputSchema: {
        type: "object",
        properties: {
          html: { type: "string", description: "the HTML to render" },
          css: { type: "string", description: "optional CSS stylesheet" },
          width: { type: "number", description: "canvas width in pixels (default 800)" },
          height: { type: "number", description: "canvas height in pixels (default 600)" },
        },
        required: ["html"],
      },
    },
  ],
}));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  if (req.params.name !== "render_html") {
    return { isError: true, content: [{ type: "text", text: `unknown tool: ${req.params.name}` }] };
  }
  const args = req.params.arguments || {};
  if (typeof args.html !== "string" || args.html.length === 0) {
    return { isError: true, content: [{ type: "text", text: "html must be a non-empty string" }] };
  }
  const css = typeof args.css === "string" ? args.css : "";
  const width = clampDim(args.width, 800);
  const height = clampDim(args.height, 600);
  try {
    const base64 = await snapshot(args.html, css, width, height);
    return {
      content: [
        { type: "image", data: base64, mimeType: "image/png" },
        { type: "text", text: `rendered ${width}x${height} PNG` },
      ],
    };
  } catch (e) {
    return { isError: true, content: [{ type: "text", text: e.message }] };
  }
});

const transport = new StdioServerTransport();
await server.connect(transport);
