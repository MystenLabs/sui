// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';

const ELLIPSIS = '\u{2026}';

export function formatAddress(address: string) {
	if (address.length <= 6) {
		return address;
	}

	const offset = address.startsWith('0x') ? 2 : 0;

	return `0x${address.slice(offset, offset + 4)}${ELLIPSIS}${address.slice(-4)}`;
}

export function formatDigest(digest: string) {
	// Use 10 first characters
	return `${digest.slice(0, 10)}${ELLIPSIS}`;
}

export function formatAmountParts(amount?: BigNumber | bigint | number | string | null) {
	if (typeof amount === 'undefined' || amount === null) {
		return ['--'];
	}

	let postfix = '';
	let bn = new BigNumber(amount.toString());
	const bnAbs = bn.abs();

	// use absolute value to determine the postfix
	if (bnAbs.gte(1_000_000_000)) {
		bn = bn.shiftedBy(-9);
		postfix = 'B';
	} else if (bnAbs.gte(1_000_000)) {
		bn = bn.shiftedBy(-6);
		postfix = 'M';
	} else if (bnAbs.gte(10_000)) {
		bn = bn.shiftedBy(-3);
		postfix = 'K';
	}

	if (bnAbs.gte(1)) {
		bn = bn.decimalPlaces(2, BigNumber.ROUND_DOWN);
	}

	return [bn.toFormat(), postfix];
}

export function formatAmount(...args: Parameters<typeof formatAmountParts>) {
	return formatAmountParts(...args)
		.filter(Boolean)
		.join(' ');
}
