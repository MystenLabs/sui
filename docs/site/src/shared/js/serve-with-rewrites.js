/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Local server that serves the Docusaurus build with proper headers
 * for markdown files, llms.txt, and content negotiation.
 */

const http = require('http');
const fs = require('fs');
const path = require('path');
const url = require('url');

const PORT = process.argv[2] || 3001;
const BUILD_DIR = path.join(__dirname, '../../../build');

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
  '.txt': 'text/plain; charset=utf-8',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.pdf': 'application/pdf',
};

function getContentType(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  return MIME_TYPES[ext] || 'application/octet-stream';
}

function getCacheControl(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  if (ext === '.html' || ext === '.txt' || ext === '.md') {
    return 'public, max-age=0, must-revalidate';
  }
  return 'public, max-age=3600';
}

/**
 * Checks whether the request Accept header includes text/markdown.
 */
function acceptsMarkdown(req) {
  const accept = req.headers['accept'] || '';
  return accept.includes('text/markdown');
}

/**
 * Tries to resolve a markdown file for the given pathname.
 * Maps e.g. "/" → "markdown/index.md", "/developer" → "markdown/developer.md",
 * "/developer/sdk" → "markdown/developer/sdk.md".
 */
function resolveMarkdownFile(pathname) {
  const clean = pathname.replace(/\/+$/, '') || '/';

  // Direct .md path inside markdown/
  if (clean === '/') {
    const candidate = path.join(BUILD_DIR, 'markdown', 'index.md');
    if (fs.existsSync(candidate)) return candidate;
    return null;
  }

  // Try <path>.md first, then <path>/index.md
  const asMd = path.join(BUILD_DIR, 'markdown', clean + '.md');
  if (fs.existsSync(asMd)) return asMd;

  const asIndex = path.join(BUILD_DIR, 'markdown', clean, 'index.md');
  if (fs.existsSync(asIndex)) return asIndex;

  return null;
}

const server = http.createServer((req, res) => {
  const parsedUrl = url.parse(req.url);
  let pathname = parsedUrl.pathname;

  // Content negotiation: serve markdown when Accept: text/markdown
  if (acceptsMarkdown(req)) {
    const mdFile = resolveMarkdownFile(pathname);
    if (mdFile) {
      const content = fs.readFileSync(mdFile);
      res.writeHead(200, {
        'Content-Type': 'text/markdown; charset=utf-8',
        'Content-Disposition': 'inline',
        'Cache-Control': 'public, max-age=0, must-revalidate',
      });
      res.end(content);
      return;
    }
  }

  // Resolve file path
  let filePath = path.join(BUILD_DIR, pathname);

  if (!fs.existsSync(filePath)) {
    // Try index.html for directory-style routes
    const indexPath = path.join(BUILD_DIR, pathname, 'index.html');
    if (fs.existsSync(indexPath)) {
      filePath = indexPath;
    } else {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('404 Not Found');
      return;
    }
  } else if (fs.statSync(filePath).isDirectory()) {
    filePath = path.join(filePath, 'index.html');
  }

  if (!fs.existsSync(filePath)) {
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    res.end('404 Not Found');
    return;
  }

  const content = fs.readFileSync(filePath);
  res.writeHead(200, {
    'Content-Type': getContentType(filePath),
    'Content-Disposition': 'inline',
    'Cache-Control': getCacheControl(filePath),
  });
  res.end(content);
});

server.listen(PORT, () => {
  console.log(`Serving build at http://localhost:${PORT}/`);
});
