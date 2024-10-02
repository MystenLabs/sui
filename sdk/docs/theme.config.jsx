// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRouter } from 'next/router';

const config = {
	logo: <span>Sui TypeScript Docs</span>,
	project: {
		link: 'https://github.com/MystenLabs/sui/tree/main/sdk/',
	},
	chat: {
		link: 'https://discord.com/invite/Sui',
	},
	docsRepositoryBase: 'https://github.com/MystenLabs/sui/tree/main/sdk/docs',
	footer: {
		text: `Copyright © ${new Date().getFullYear()}, Mysten Labs, Inc.`,
	},
	head: (
		<>
			<meta name="google-site-verification" content="T-2HWJAKh8s63o9KFxCFXg5MON_NGLJG76KJzr_Hp0A" />
			<meta httpEquiv="Content-Language" content="en" />
		</>
	),
	banner: {
		key: '1.0-release',
		dismissible: false,
		text: (
			<a href="/typescript/migrations/sui-1.0">
				🎉 @mysten/sui 1.0 has been released - Read the full migration guide here!
			</a>
		),
	},
	useNextSeoProps() {
		const { asPath } = useRouter();

		return {
			titleTemplate: asPath !== '/' ? '%s | Sui TypeScript Docs' : 'Sui TypeScript Docs',
			description:
				'Sui TypeScript Documentation. Discover the power of Sui through examples, guides, and concepts.',
			openGraph: {
				title: 'Sui TypeScript Docs',
				description:
					'Sui TypeScript Documentation. Discover the power of Sui through examples, guides, and concepts.',
				site_name: 'Sui TypeScript Docs',
			},
			additionalMetaTags: [{ content: 'Sui TypeScript Docs', name: 'apple-mobile-web-app-title' }],
			twitter: {
				card: 'summary_large_image',
				site: '@Mysten_Labs',
			},
		};
	},
};

export default config;
