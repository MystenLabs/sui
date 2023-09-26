// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

export const overlay = style({
	backgroundColor: 'rgba(24 36 53 / 20%)',
	position: 'fixed',
	inset: 0,
	zIndex: 999999999,
});

export const title = style({
	paddingLeft: 8,
});

export const content = style({
	backgroundColor: 'white',
	position: 'fixed',
	bottom: 16,
	left: 16,
	right: 16,
	zIndex: 999999999,
	display: 'flex',
	flexDirection: 'column',
	justifyContent: 'space-between',
	overflow: 'hidden',
	borderRadius: 16,
	minHeight: '50vh',
	maxHeight: '85vh',
	maxWidth: 700,
	'@media': {
		'screen and (min-width: 768px)': {
			flexDirection: 'row',
			width: '100%',
			top: '50%',
			left: '50%',
			transform: 'translate(-50%, -50%)',
		},
	},
});

export const whatIsAWalletButton = style({
	backgroundColor: '#F7F8F8',
	padding: 16,
	'@media': {
		'screen and (min-width: 768px)': {
			display: 'none',
		},
	},
});

export const viewContainer = style({
	display: 'none',
	padding: 20,
	flexGrow: 1,
	'@media': {
		'screen and (min-width: 768px)': {
			display: 'flex',
		},
	},
});

export const selectedViewContainer = style({
	display: 'flex',
});

export const triggerButton = style({
	backgroundColor: '#0284AD',
	boxShadow: '0px 4px 12px rgba(0, 0, 0, 0.1)',
	color: 'white',
	borderRadius: 12,
	paddingLeft: 24,
	paddingRight: 24,
	paddingTop: 16,
	paddingBottom: 16,
	':hover': {
		backgroundColor: '#007194',
	},
});

export const backButton = style({
	position: 'absolute',
	top: 20,
	left: 20,
	'@media': {
		'screen and (min-width: 768px)': {
			display: 'none',
		},
	},
});

export const closeButton = style({
	position: 'absolute',
	padding: 7,
	top: 16,
	right: 16,
	borderRadius: 9999,
	backgroundColor: '#F0F1F2',
});

export const walletListContent = style({
	padding: 20,
	minWidth: 240,
	display: 'flex',
	flexDirection: 'column',
	gap: 24,
	'@media': {
		'screen and (min-width: 768px)': {
			backgroundColor: '#F7F8F8',
		},
	},
});

export const walletListContainer = style({
	display: 'flex',
	justifyContent: 'space-between',
	flexDirection: 'column',
	flexGrow: 1,
	'@media': {
		'screen and (min-width: 768px)': {
			flexDirection: 'row',
			flexGrow: 0,
		},
	},
});

export const walletListContainerWithViewSelected = style({
	display: 'none',
	'@media': {
		'screen and (min-width: 768px)': {
			display: 'flex',
		},
	},
});
