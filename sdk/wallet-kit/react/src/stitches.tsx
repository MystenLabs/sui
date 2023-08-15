// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createStitches } from '@stitches/react';

const BASE_UNIT = 4;
const makeSize = (amount: number) => `${amount * BASE_UNIT}px`;

export const { styled, css, globalCss, keyframes, getCssText, theme, createTheme, config } =
	createStitches({
		media: {
			sm: '(min-width: 640px)',
			md: '(min-width: 768px)',
			lg: '(min-width: 1024px)',
		},
		theme: {
			colors: {
				brand: '#0284AD',
				brandAccent: '#007195',
				secondary: '#C3C5C8',
				secondaryAccent: '#636870',
				textDark: '#182435',
				textLight: '#767A81',
				textOnBrand: '#fff',
				background: '#fff',
				backgroundAccent: '#F7F8F8',
				backdrop: 'rgba(24 36 53 / 20%)',
				backgroundIcon: '#F0F1F2',
				icon: '#383F47',
				issue: '#FF794B',
			},
			space: {
				1: makeSize(1),
				2: makeSize(2),
				3: makeSize(3),
				4: makeSize(4),
				5: makeSize(5),
				6: makeSize(6),
				7: makeSize(7),
				8: makeSize(8),
				10: makeSize(10),
			},
			fontSizes: {
				xs: '13px',
				sm: '14px',
				md: '16px',
				lg: '18px',
				xl: '20px',
			},
			fonts: {
				sans: 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji"',
				mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
			},
			radii: {
				modal: '16px',
				buttonLg: '12px',
				buttonMd: '8px',
				wallet: '8px',
				close: '9999px',
			},
			fontWeights: {
				copy: 500,
				button: 600,
				title: 600,
			},
			transitions: {},
			shadows: {
				button: '0px 4px 12px rgba(0, 0, 0, 0.1)',
				modal: '0px 0px 44px rgba(0, 0, 0, 0.15)',
				wallet: '0px 2px 6px rgba(0, 0, 0, 0.05)',
			},
		},
	});
