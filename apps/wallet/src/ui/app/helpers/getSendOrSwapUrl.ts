// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export function getSendOrSwapUrl(page: 'send' | 'swap', coinType: string, quoteAsset?: string) {
	const encodedCoinType = encodeURIComponent(coinType);
	const encodedQuoteAsset = quoteAsset ? encodeURIComponent(quoteAsset) : undefined;

	const path = `/${page}?type=${encodedCoinType}`;

	return encodedQuoteAsset ? `${path}&quoteAsset=${encodedQuoteAsset}` : path;
}
