#!/usr/bin/env node

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Local development server that handles Vercel rewrites and content negotiation.
 * Supports:
 * - Accept: text/markdown content negotiation (returns markdown when requested)
 * - .md URL rewrites (e.g. /develop.md → /markdown/develop.md)
 * - Proper response headers (Content-Type, Vary, Cache-Control, Link)
 *
 * Usage:
 *   node scripts/serve-with-rewrites.js [--port PORT]
 */

const http = require('http');
const fs = require('fs');
const path = require('path');
const url = require('url');

const args = process.argv.slice(2);
let PORT = parseInt(process.env.PORT, 10) || 3000;
let buildDir = path.join(__dirname, '../build');
for (let i = 0; i < args.length; i++) {
  if ((args[i] === '--port' || args[i] === '-p') && args[i + 1]) {
    PORT = parseInt(args[i + 1], 10);
  }
  if ((args[i] === '--dir' || args[i] === '-d') && args[i + 1]) {
    buildDir = path.resolve(args[i + 1]);
  }
}

const BUILD_DIR = buildDir;

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.gif': 'image/gif',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.md': 'text/markdown; charset=utf-8',
  '.txt': 'text/plain',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.xml': 'application/xml',
};

const LINK_HEADER =
  '</.well-known/mcp/server-card.json>; rel="mcp-server-card"; type="application/json", ' +
  '</.well-known/api-catalog>; rel="api-catalog", ' +
  '</llms.txt>; rel="service-doc"; type="text/plain"; title="LLM-optimized documentation", ' +
  '</sitemap.xml>; rel="sitemap"; type="application/xml", ' +
  '</robots.txt>; rel="robots"; type="text/plain", ' +
  '</references/sui-api>; rel="service-doc"; title="Sui API Reference"';

function getContentType(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  return MIME_TYPES[ext] || 'application/octet-stream';
}

function serveFile(res, filePath, extraHeaders = {}) {
  fs.readFile(filePath, (err, content) => {
    if (err) {
      if (err.code === 'ENOENT') {
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('404 Not Found');
      } else {
        res.writeHead(500, { 'Content-Type': 'text/plain' });
        res.end('500 Internal Server Error');
      }
    } else {
      const contentType = getContentType(filePath);
      res.writeHead(200, {
        'Content-Type': contentType,
        'Cache-Control': 'public, max-age=3600',
        'Link': LINK_HEADER,
        'Vary': 'Accept',
        ...extraHeaders,
      });
      res.end(content, 'utf-8');
    }
  });
}

function serveMarkdown(res, pathname) {
  let mdPath = pathname;

  // Strip trailing slash
  if (mdPath.length > 1 && mdPath.endsWith('/')) {
    mdPath = mdPath.slice(0, -1);
  }

  // Root maps to index
  if (mdPath === '' || mdPath === '/') {
    mdPath = '/index';
  }

  // Try direct file first, then index file
  const candidates = [
    path.join(BUILD_DIR, 'markdown', mdPath + '.md'),
    path.join(BUILD_DIR, 'markdown', mdPath, 'index.md'),
  ];
  const filePath = candidates.find(f => fs.existsSync(f));

  if (filePath) {
    const content = fs.readFileSync(filePath, 'utf-8');
    const tokens = Math.ceil(content.length / 4);
    const byteLength = Buffer.byteLength(content, 'utf-8');

    res.writeHead(200, {
      'Content-Type': 'text/markdown; charset=utf-8',
      'Content-Disposition': 'inline',
      'Content-Length': String(byteLength),
      'Cache-Control': 'public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400',
      'Vary': 'Accept',
      'Link': LINK_HEADER,
      'x-markdown-tokens': String(tokens),
    });
    res.end(content, 'utf-8');
    return true;
  }

  return false;
}

// File extensions that should never trigger content negotiation
const STATIC_EXTENSIONS = new Set([
  '.md', '.json', '.xml', '.txt', '.js', '.css',
  '.png', '.jpg', '.jpeg', '.gif', '.svg', '.ico',
  '.woff', '.woff2', '.pdf',
]);

const server = http.createServer((req, res) => {
  const parsedUrl = url.parse(req.url);
  let pathname = decodeURIComponent(parsedUrl.pathname);

  // Content negotiation: Accept: text/markdown
  const accept = req.headers['accept'] || '';
  const ext = path.extname(pathname).toLowerCase();

  if (accept.includes('text/markdown') && !STATIC_EXTENSIONS.has(ext)) {
    if (serveMarkdown(res, pathname)) return;
    // Fall through to HTML if markdown doesn't exist
  }

  // Handle .md URL requests (mimics Vercel rewrite: /path.md → /markdown/path.md)
  if (pathname.endsWith('.md') && !pathname.startsWith('/markdown/')) {
    const mdPath = pathname.replace(/\.md$/, '');
    const mdCandidates = [
      path.join(BUILD_DIR, 'markdown', mdPath + '.md'),
      path.join(BUILD_DIR, 'markdown', mdPath, 'index.md'),
    ];
    const filePath = mdCandidates.find(f => fs.existsSync(f));

    if (filePath) {
      const content = fs.readFileSync(filePath, 'utf-8');
      const tokens = Math.ceil(content.length / 4);
      const byteLength = Buffer.byteLength(content, 'utf-8');

      res.writeHead(200, {
        'Content-Type': 'text/markdown; charset=utf-8',
        'Content-Disposition': 'inline',
        'Content-Length': String(byteLength),
        'Cache-Control': 'public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400',
        'Vary': 'Accept',
        'Link': LINK_HEADER,
        'x-markdown-tokens': String(tokens),
      });
      res.end(content, 'utf-8');
      return;
    }
  }

  // Handle llms.txt with special cache headers
  if (pathname === '/llms.txt') {
    const filePath = path.join(BUILD_DIR, 'llms.txt');
    if (fs.existsSync(filePath)) {
      serveFile(res, filePath, {
        'Cache-Control': 'public, max-age=0, must-revalidate',
      });
      return;
    }
  }

  // Serve static files
  let filePath = path.join(BUILD_DIR, pathname);

  if (!fs.existsSync(filePath)) {
    filePath = path.join(BUILD_DIR, pathname, 'index.html');
    if (!fs.existsSync(filePath)) {
      filePath = path.join(BUILD_DIR, 'index.html');
    }
  } else if (fs.statSync(filePath).isDirectory()) {
    filePath = path.join(filePath, 'index.html');
  }

  serveFile(res, filePath);
});

server.listen(PORT, () => {
  console.log(`\n🚀 Local server with content negotiation running on http://localhost:${PORT}`);
  console.log(`\n📖 Test URLs:`);
  console.log(`   HTML:      http://localhost:${PORT}/develop`);
  console.log(`   .md URL:   http://localhost:${PORT}/develop.md`);
  console.log(`   Negotiate: curl -H "Accept: text/markdown" http://localhost:${PORT}/develop`);
  console.log(`\n💡 Press Ctrl+C to stop\n`);
});
