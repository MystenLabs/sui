# Markdown Export Feature

## Overview

The Sui documentation now supports viewing raw markdown versions of any documentation page by appending `.md` to the URL. This makes the documentation LLM-ready and easier to consume programmatically.

## Usage

### Viewing Markdown

Simply add `.md` to any documentation URL:

```
HTML version: https://docs.sui.io/guides/developer/getting-started
Markdown version: https://docs.sui.io/guides/developer/getting-started.md
```

### Examples

| Page Type | HTML URL | Markdown URL |
|-----------|----------|--------------|
| Guide | `https://docs.sui.io/guides/developer/sui-101` | `https://docs.sui.io/guides/developer/sui-101.md` |
| Concept | `https://docs.sui.io/concepts/architecture` | `https://docs.sui.io/concepts/architecture.md` |
| Reference | `https://docs.sui.io/references/cli` | `https://docs.sui.io/references/cli.md` |
| Standard | `https://docs.sui.io/standards/kiosk` | `https://docs.sui.io/standards/kiosk.md` |

## Implementation Details

### Architecture

The markdown export feature consists of three components:

1. **Build Script** (`scripts/copy-markdown-files.js`)
   - Runs during the build process
   - Copies all `.md` and `.mdx` files from `/docs/content/`
   - Strips frontmatter (YAML metadata)
   - Outputs to `/build/markdown/` directory

2. **Vercel Rewrites** (`vercel.json`)
   - Rewrites requests ending in `.md` to `/markdown/{path}.md`
   - Serves static markdown files from the build output

3. **Headers Configuration** (`vercel.json`)
   - Sets `Content-Type: text/markdown; charset=utf-8`
   - Sets `Content-Disposition: inline` (display in browser, not download)
   - Configures 1-hour cache for performance

### Build Process

```bash
# During build, the following happens:
1. Docusaurus builds HTML pages → /build/
2. Copy script exports markdown → /build/markdown/
3. Vercel deploys both HTML and markdown files
```

### File Structure

```
/build/
├── index.html                    # HTML pages
├── guides/
│   └── getting-started.html
└── markdown/                     # Markdown exports
    ├── guides/
    │   └── getting-started.md
    ├── concepts/
    └── references/
```

## Development

### Testing Locally

```bash
# Build the docs
cd docs/site
pnpm build

# Test the markdown export
node scripts/test-markdown-export.js

# Serve locally
pnpm serve

# Visit markdown URL
# http://localhost:3000/guides/getting-started.md
```

### Adding New Documentation

No special steps required. When you add new `.md` or `.mdx` files to `/docs/content/`, they will automatically be included in the markdown export during the next build.

### Troubleshooting

**Problem:** Markdown file not found (404)

**Solutions:**
1. Ensure the HTML version of the page exists first
2. Check that the file exists in `/docs/content/`
3. Rebuild the docs: `pnpm build`
4. Verify the file was copied: check `/build/markdown/` directory

**Problem:** Frontmatter still visible

**Solution:** The `gray-matter` package should strip frontmatter automatically. If you see frontmatter, check the `stripFrontmatter()` function in `scripts/copy-markdown-files.js`.

**Problem:** Build fails

**Solution:** Check that `gray-matter` is installed:
```bash
pnpm install gray-matter
```

## Benefits

### For LLMs (Language Models)
- Clean markdown without HTML/React components
- No navigation or UI elements
- Direct access to content for training/RAG systems

### For Developers
- Easy to curl/wget documentation
- Programmatic access to docs
- Simple integration with scripts and tools

### For Documentation Tools
- Compatible with documentation aggregators
- Easy to index and search
- Standard markdown format

## Maintenance

The markdown export is fully automated and requires no manual maintenance. It runs as part of the standard build process and stays in sync with the HTML documentation automatically.

## Similar Implementations

This feature is inspired by:
- **Solana Docs:** https://solana.com/docs/core.md
- **Hyperliquid Docs:** Uses GitBook's built-in markdown export

## Future Enhancements

Potential improvements:
- [ ] Add API endpoint to list all available markdown files
- [ ] Support query parameters for custom formatting
- [ ] Add RSS feed of documentation updates
- [ ] Generate OpenAPI spec from markdown structure
