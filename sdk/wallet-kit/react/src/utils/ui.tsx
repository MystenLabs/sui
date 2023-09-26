// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { styled } from '../stitches';

export const Button = styled('button', {
	cursor: 'pointer',
	border: 'none',
	fontFamily: '$sans',
	fontWeight: '$button',
	fontSize: '$sm',
	textDecoration: 'none',

	variants: {
		size: {
			md: {
				padding: '$2 $4',
				borderRadius: '$buttonMd',
			},
			lg: {
				padding: '$4 $6',
				borderRadius: '$buttonLg',
			},
		},
		color: {
			primary: {
				backgroundColor: '$brand',
				color: '$textOnBrand',
				'&:hover': {
					backgroundColor: '$brandAccent',
				},
				boxShadow: '$button',
			},
			secondary: {
				backgroundColor: 'transparent',
				border: '1px solid $secondary',
				color: '$secondaryAccent',
			},
			connected: {
				boxShadow: '$button',
				backgroundColor: '$background',
				color: '$textDark',
			},
		},
	},
	defaultVariants: {
		size: 'md',
	},
});

export const Panel = styled('div', {
	flex: 1,
	boxSizing: 'border-box',
	padding: '$5',
	display: 'flex',
	flexDirection: 'column',

	variants: {
		responsiveHidden: {
			true: {
				display: 'none',
				'@md': { display: 'flex' },
			},
		},
	},
});

export const Truncate = styled('div', {
	overflow: 'hidden',
	textOverflow: 'ellipsis',
	whiteSpace: 'nowrap',
});

export const CopyContainer = styled('div', {
	display: 'flex',
	flexDirection: 'column',
	gap: '$5',
	marginBottom: '$4',
});

export const Heading = styled('h3', {
	color: '$textDark',
	fontSize: '$sm',
	margin: 0,
	marginBottom: '$1',
});

export const Description = styled('div', {
	color: '$textLight',
	fontSize: '$sm',
	fontWeight: '$copy',
	lineHeight: '1.3',
});
