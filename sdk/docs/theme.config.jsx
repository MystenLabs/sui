// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const config = {
	logo: <span>Sui Typescript Docs</span>,
	project: {
		link: 'https://github.com/MystenLabs/sui/tree/main/sdk/typescript',
	},
	docsRepositoryBase: 'https://github.com/MystenLabs/sui/tree/main/sdk/docs/pages',
	useNextSeoProps() {
		return {
			titleTemplate: '%s',
		};
	},
};

export default config;
