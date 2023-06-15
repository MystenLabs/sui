// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DocsThemeConfig } from 'nextra-theme-docs';

const config: DocsThemeConfig = {
	logo: <span>Sui Wallet Kit</span>,
	project: {
		link: 'https://github.com/MystenLabs/sui/tree/main/sdk/wallet-adapter',
	},
	chat: {
		link: 'https://discord.com/invite/Sui',
	},
	docsRepositoryBase: 'https://github.com/MystenLabs/sui/tree/main/sdk/wallet-adapter',
	footer: {
		text: 'Copyright © 2023, Mysten Labs, Inc.',
	},
	useNextSeoProps() {
		return {
			titleTemplate: '%s – Sui Wallet Kit',
		};
	},
};

export default config;
