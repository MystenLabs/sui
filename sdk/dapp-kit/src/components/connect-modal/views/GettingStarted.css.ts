// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { style } from '@vanilla-extract/css';

export const container = style({
	display: 'flex',
	flexDirection: 'column',
	alignItems: 'center',
});

export const content = style({
	display: 'flex',
	flexDirection: 'column',
	justifyContent: 'center',
	flexGrow: 1,
	gap: 20,
	padding: 40,
});

export const installButtonContainer = style({
	position: 'absolute',
	bottom: 20,
	right: 20,
});
