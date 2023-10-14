// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

import { themeVars } from '../../themes/themeContract.js';

export const overlay = style({
	backgroundColor: themeVars.backgroundColors.modalOverlay,
	backdropFilter: themeVars.blurs.modalOverlay,
	position: 'fixed',
	inset: 0,
	zIndex: 999999999,
});

export const title = style({
	paddingLeft: 8,
});

export const content = style({
	backgroundColor: themeVars.backgroundColors.modalPrimary,
	borderRadius: themeVars.radii.xlarge,
	color: themeVars.colors.body,
	position: 'fixed',
	bottom: 16,
	left: 16,
	right: 16,
	display: 'flex',
	flexDirection: 'column',
	justifyContent: 'space-between',
	overflow: 'hidden',
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
	backgroundColor: themeVars.backgroundColors.modalSecondary,
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

export const backButtonContainer = style({
	position: 'absolute',
	top: 20,
	left: 20,
	'@media': {
		'screen and (min-width: 768px)': {
			display: 'none',
		},
	},
});

export const closeButtonContainer = style({
	position: 'absolute',
	top: 16,
	right: 16,
});

export const walletListContent = style({
	display: 'flex',
	flexDirection: 'column',
	flexGrow: 1,
	gap: 24,
	padding: 20,
	backgroundColor: themeVars.backgroundColors.modalPrimary,
	'@media': {
		'screen and (min-width: 768px)': {
			backgroundColor: themeVars.backgroundColors.modalSecondary,
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
			flexBasis: 240,
			flexGrow: 0,
			flexShrink: 0,
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
