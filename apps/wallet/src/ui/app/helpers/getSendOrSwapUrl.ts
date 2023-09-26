// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export function getSendOrSwapUrl(page: 'send' | 'swap', coinType: string) {
	const encodedCoinType = encodeURIComponent(coinType);

	return `/${page}?type=${encodedCoinType}`;
}
