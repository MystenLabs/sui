// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export const onChainAmountToFloat = (amount: string, decimals: number) => {
	const total = parseFloat(amount);

	return total / Math.pow(10, decimals);
};

export const formatAddress = (address: string) => {
	return `${address.substring(0, 4)}...${address.slice(-10)}`;
};
