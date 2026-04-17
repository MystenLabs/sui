// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Vercel Edge Middleware: content negotiation for Markdown for Agents.
// When a request includes Accept: text/markdown, rewrite to the pre-built
// markdown version of the page. Browsers get HTML as usual.
// See: https://developers.cloudflare.com/fundamentals/reference/markdown-for-agents/

export const config = {
	// Only run on doc pages, not static assets or API routes
	matcher: [
		'/((?!_next|api|static|img|fonts|doc|paper|display-preview|markdown|llms\\.txt|robots\\.txt|sitemap\\.xml|favicon).*)',
	],
};

export default function middleware(request) {
	const accept = request.headers.get('accept') || '';

	// Check if the client prefers markdown over HTML
	if (!accept.includes('text/markdown')) {
		return; // Let the default HTML response through
	}

	const url = new URL(request.url);
	let path = url.pathname;

	// Skip paths that are already markdown, static files, or non-page routes
	if (
		path.endsWith('.md') ||
		path.endsWith('.json') ||
		path.endsWith('.xml') ||
		path.endsWith('.txt') ||
		path.endsWith('.js') ||
		path.endsWith('.css') ||
		path.endsWith('.png') ||
		path.endsWith('.jpg') ||
		path.endsWith('.svg') ||
		path.endsWith('.ico') ||
		path.endsWith('.woff2') ||
		path.endsWith('.pdf')
	) {
		return;
	}

	// Strip trailing slash for consistency
	if (path.length > 1 && path.endsWith('/')) {
		path = path.slice(0, -1);
	}

	// Root path maps to the index
	if (path === '' || path === '/') {
		path = '/index';
	}

	// Rewrite to the markdown version
	const markdownUrl = new URL(`/markdown${path}.md`, request.url);

	return fetch(markdownUrl, {
		headers: {
			...Object.fromEntries(request.headers),
			// Override accept so the downstream doesn't loop
			accept: '*/*',
		},
	}).then((response) => {
		if (!response.ok) {
			// Markdown version doesn't exist for this path, fall through to HTML
			return;
		}

		// Return the markdown with proper headers
		const headers = new Headers(response.headers);
		headers.set('content-type', 'text/markdown; charset=utf-8');
		headers.set('vary', 'Accept');

		// Token count hint for agents (approximate: 1 token ≈ 4 chars)
		const contentLength = headers.get('content-length');
		if (contentLength) {
			const tokens = Math.ceil(parseInt(contentLength) / 4);
			headers.set('x-markdown-tokens', String(tokens));
		}

		return new Response(response.body, {
			status: 200,
			headers,
		});
	});
}
