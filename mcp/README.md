# Kaleidoscope MCP server

An [MCP](https://modelcontextprotocol.io) server that renders HTML and CSS to a PNG with the Kaleidoscope engine. It gives an AI agent a browser-free way to see what a snippet of HTML and CSS looks like.

A headless Chromium takes hundreds of megabytes of memory and seconds to boot. Kaleidoscope boots in milliseconds and uses a couple of megabytes, so an agent can generate markup, render it, and look at the result in a tight loop.

## Tool

- **render_html** render an HTML string with an optional CSS string to a PNG and return the image. Arguments: `html` (required), `css`, `width` (default 800), `height` (default 600). The response includes the PNG as an image plus a short text note.

## Install

Build and install the binary so the server can call it:

```
cargo install --path ..
```

Install the server dependencies:

```
npm install
```

## Configure your client

Point your MCP client at `index.js`. For Claude Code:

```
claude mcp add kaleidoscope -- node /absolute/path/to/kaleidoscope/mcp/index.js
```

If the `kaleidoscope` binary is not on `PATH`, set `KALEIDOSCOPE_BIN` to its full path in the server environment.

## Example flow for an agent

1. Generate some HTML and CSS.
2. Call `render_html` with them.
3. Look at the returned PNG, adjust the markup, and render again.

By Pavan Nallamothu.
