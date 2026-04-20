// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// WebMCP: expose documentation tools to AI agents via the browser.
// https://webmachinelearning.github.io/webmcp/

if (typeof window !== 'undefined') {
	const registerTools = () => {
		if (!navigator.modelContext?.provideContext) return;

		navigator.modelContext.provideContext({
			tools: [
				{
					name: 'search_sui_docs',
					description:
						'Search the Sui documentation site for pages matching a query. Returns page titles, URLs, and snippets.',
					inputSchema: {
						type: 'object',
						properties: {
							query: {
								type: 'string',
								description: 'The search query to find relevant documentation pages.',
							},
						},
						required: ['query'],
					},
					async execute({ query }) {
						// Use the Docusaurus search index if available
						const searchUrl = `${window.location.origin}/search?q=${encodeURIComponent(query)}`;
						return {
							type: 'text',
							text: `Search Sui docs for "${query}": ${searchUrl}`,
						};
					},
				},
				{
					name: 'get_page_content',
					description:
						'Get the full markdown content of the current documentation page. Useful for reading and understanding the page the user is viewing.',
					inputSchema: {
						type: 'object',
						properties: {},
					},
					async execute() {
						const path = window.location.pathname;
						// Fetch the markdown version of the current page
						const mdPath = path.endsWith('/') ? path.slice(0, -1) : path;
						try {
							const resp = await fetch(`${window.location.origin}/markdown${mdPath}.md`);
							if (resp.ok) {
								const text = await resp.text();
								return { type: 'text', text };
							}
						} catch (e) {
							// Fall through
						}
						// Fallback: extract text from the article element
						const article = document.querySelector('article') || document.querySelector('main');
						return {
							type: 'text',
							text: article ? article.innerText : document.body.innerText.slice(0, 10000),
						};
					},
				},
				{
					name: 'get_page_metadata',
					description:
						'Get metadata about the current documentation page including title, description, URL, and table of contents.',
					inputSchema: {
						type: 'object',
						properties: {},
					},
					async execute() {
						const title = document.title;
						const description =
							document.querySelector('meta[name="description"]')?.content || '';
						const url = window.location.href;

						// Extract table of contents
						const tocLinks = document.querySelectorAll('.table-of-contents__link');
						const toc = Array.from(tocLinks).map((link) => ({
							text: link.textContent.trim(),
							id: link.getAttribute('href')?.replace('#', '') || '',
						}));

						// Extract breadcrumbs for navigation context
						const breadcrumbs = Array.from(
							document.querySelectorAll('.breadcrumbs__link'),
						).map((el) => el.textContent.trim());

						return {
							type: 'text',
							text: JSON.stringify(
								{ title, description, url, breadcrumbs, toc },
								null,
								2,
							),
						};
					},
				},
				{
					name: 'list_sidebar_pages',
					description:
						'List all pages in the current documentation section from the sidebar navigation. Useful for understanding what topics are covered.',
					inputSchema: {
						type: 'object',
						properties: {},
					},
					async execute() {
						const links = document.querySelectorAll('.menu__link');
						const pages = Array.from(links).map((link) => ({
							title: link.textContent.trim(),
							href: link.getAttribute('href') || '',
							active: link.classList.contains('menu__link--active'),
						}));
						return {
							type: 'text',
							text: JSON.stringify(pages, null, 2),
						};
					},
				},
				{
					name: 'get_sui_api_reference',
					description:
						'Get a summary of available Sui APIs including JSON-RPC, GraphQL, and gRPC endpoints with their documentation URLs.',
					inputSchema: {
						type: 'object',
						properties: {},
					},
					async execute() {
						try {
							const resp = await fetch('/.well-known/api-catalog');
							if (resp.ok) {
								const catalog = await resp.json();
								return { type: 'text', text: JSON.stringify(catalog, null, 2) };
							}
						} catch (e) {
							// Fall through
						}
						return {
							type: 'text',
							text: JSON.stringify({
								apis: [
									{
										name: 'Sui JSON-RPC',
										docs: 'https://docs.sui.io/references/sui-api',
										spec: 'https://docs.sui.io/open-spec/mainnet/openrpc.json',
									},
									{
										name: 'Sui GraphQL',
										docs: 'https://docs.sui.io/references/sui-graphql',
										endpoint: 'https://graphql.mainnet.sui.io/graphql',
									},
								],
							}),
						};
					},
				},
			],
		});
	};

	// Register on load, and re-register on client-side navigation
	if (document.readyState === 'complete') {
		registerTools();
	} else {
		window.addEventListener('load', registerTools);
	}
}
