// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export const DEEPBOOK_KEY = 'deepbook';
export const SUI_CONVERSION_RATE = 6;
export const USDC_DECIMALS = 9;
export const MAX_FLOAT = 2;
export const WALLET_FEES_PERCENTAGE = 0.5;
export const ESTIMATED_GAS_FEES_PERCENTAGE = 1;
export const ONE_SUI_DEEPBOOK = 1000000000;
export const DEFAULT_WALLET_FEE_ADDRESS =
	'0x55b0eb986766351d802ac3e1bbb8750a072b3fa40c782ebe3a0f48c9099f7fd3';
export const DEFAULT_MAX_SLIPPAGE_PERCENTAGE = '0.5';
export const initialValues = {
	amount: '',
	toAssetType: '',
	allowedMaxSlippagePercentage: DEFAULT_MAX_SLIPPAGE_PERCENTAGE,
};
export type FormValues = typeof initialValues;
