// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export const DEEPBOOK_KEY = 'deepbook';
export const SUI_CONVERSION_RATE = 6;
export const USDC_CONVERSION_RATE = 9;
export const MAX_FLOAT = 2;
export const WALLET_FEES_PERCENTAGE = 0.5;
export const DEFAULT_WALLET_FEE_ADDRESS =
	'0x55b0eb986766351d802ac3e1bbb8750a072b3fa40c782ebe3a0f48c9099f7fd3';
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
