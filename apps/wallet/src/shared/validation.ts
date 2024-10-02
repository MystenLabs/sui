// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatBalance } from '@mysten/core';
import BigNumber from 'bignumber.js';
import * as Yup from 'yup';

export function createTokenValidation(coinBalance: bigint, coinSymbol: string, decimals: number) {
	return Yup.mixed<BigNumber>()
		.transform((_, original) => {
			return new BigNumber(original);
		})
		.test('required', `\${path} is a required field`, (value) => {
			return !!value;
		})
		.test('valid', 'The value provided is not valid.', (value) => {
			if (!value || value.isNaN() || !value.isFinite()) {
				return false;
			}
			return true;
		})
		.test('min', `\${path} must be greater than 0 ${coinSymbol}`, (amount) =>
			amount ? amount.gt(0) : false,
		)
		.test(
			'max',
			`\${path} must be less than ${formatBalance(coinBalance, decimals)} ${coinSymbol}`,
			(amount) => (amount ? amount.shiftedBy(decimals).lte(coinBalance.toString()) : false),
		)
		.test('max-decimals', `The value exceeds the maximum decimals (${decimals}).`, (amount) => {
			return amount ? amount.shiftedBy(decimals).isInteger() : false;
		})
		.label('Amount');
}
