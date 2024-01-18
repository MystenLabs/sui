// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from 'vocs';

import { sidebar } from './sidebar.js';

export default defineConfig({
	baseUrl: 'https://sdk.mystenlabs.com',
	title: 'SDK Docs',
	// TODO:
	// titleTemplate: '%s · Viem',
	// description:
	// 	'Build reliable Ethereum apps & libraries with lightweight, composable, & type-safe modules from viem.',
	// head: (
	// 	<>
	// 		<script src="https://cdn.usefathom.com/script.js" data-site="BYCJMNBD" defer />
	// 	</>
	// ),
	// TODO:
	// ogImageUrl: {
	// 	'/': '/og-image.png',
	// 	'/docs': 'https://vocs.dev/api/og?logo=%logo&title=%title&description=%description',
	// 	'/op-stack': 'https://vocs.dev/api/og?logo=%logo&title=%title&description=%description',
	// },
	// iconUrl: { light: '/favicons/light.png', dark: '/favicons/dark.png' },
	// logoUrl: { light: '/icon-light.png', dark: '/icon-dark.png' },
	rootDir: '.',
	sidebar,
	socials: [
		{
			icon: 'github',
			link: 'https://github.com/mystenlabs/sui',
		},
		// TODO:
		// {
		// 	icon: 'discord',
		// 	link: 'https://discord.gg/xCUz9FRcXD',
		// },
		// {
		// 	icon: 'x',
		// 	link: 'https://x.com/wevm_dev',
		// },
	],
	// TODO:
	theme: {
		accentColor: {
			light: '#ff9318',
			dark: '#ffc517',
		},
	},
	topNav: [
		{ text: 'TypeScript SDK', link: '/typescript' },
		{ text: 'dApp Kit', link: '/dapp-kit' },
		{ text: 'Kiosk SDK', link: '/kiosk' },
		{ text: 'zkSend SDK', link: '/zksend' },
		{ text: 'BCS', link: '/bcs' },
	],
});
