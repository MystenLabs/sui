// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { coinsMap } from '_app/hooks/useDeepBook';

export const DEFAULT_MAX_SLIPPAGE_PERCENTAGE = 0.5;
export const FEES_PERCENTAGE = 0.03;

export const initialValues = {
	amount: '',
	isPayAll: false,
	quoteAssetType: coinsMap.USDC,
	allowedMaxSlippagePercentage: DEFAULT_MAX_SLIPPAGE_PERCENTAGE,
};

export type FormValues = typeof initialValues;
