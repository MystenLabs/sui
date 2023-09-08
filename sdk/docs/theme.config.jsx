// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const config = {
	logo: <span>Sui TypeScript Docs</span>,
	project: {
		link: 'https://github.com/MystenLabs/sui/tree/main/sdk/',
	},
	chat: {
		link: 'https://discord.com/invite/Sui',
	},
	docsRepositoryBase: 'https://github.com/MystenLabs/sui/tree/main/sdk/docs/pages',
	footer: {
		text: 'Copyright © 2023, Mysten Labs, Inc.',
	},
	useNextSeoProps() {
		return {
			titleTemplate: '%s',
		};
	},
};

export default config;
