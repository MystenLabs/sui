// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export const DEEPBOOK_KEY = 'deepbook';
export const SUI_CONVERSION_RATE = 6;
export const USDC_CONVERSION_RATE = 9;
export const MAX_FLOAT = 2;
export const WALLET_FEES_PERCENTAGE = 0.5;
export const DEFAULT_WALLET_FEE_ADDRESS =
	'0x4598648c5dc4681a78618b37ae11134bfe5d2839f6bbe20a31e8bb9eb054382e';
export const DEFAULT_MAX_SLIPPAGE_PERCENTAGE = '0.5';
export const SUI_USDC_AVERAGE_CONVERSION_RATE = 3;

export const initialValues = {
	amount: '',
	toAssetType: '',
	allowedMaxSlippagePercentage: DEFAULT_MAX_SLIPPAGE_PERCENTAGE,
};
export type FormValues = typeof initialValues;

export enum Coins {
	SUI = 'SUI',
	USDC = 'USDC',
	USDT = 'USDT',
	WETH = 'WETH',
	TBTC = 'TBTC',
}
