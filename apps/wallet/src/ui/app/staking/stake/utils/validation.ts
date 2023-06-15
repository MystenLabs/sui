// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatBalance } from '@mysten/core';
import BigNumber from 'bignumber.js';
import { mixed, object } from 'yup';

export function createValidationSchema(
	coinBalance: bigint,
	coinSymbol: string,
	decimals: number,
	isUnstake: boolean,
	minimumStake: bigint,
) {
	return object({
		// NOTE: This is an intentional subset of the token validation:
		amount: isUnstake
			? mixed()
			: mixed()
					.transform((_, original) => {
						return new BigNumber(original);
					})
					.test('required', `\${path} is a required field`, (value) => {
						return !!value;
					})
					.test('valid', 'The value provided is not valid.', (value?: BigNumber) => {
						if (!value || value.isNaN() || !value.isFinite()) {
							return false;
						}
						return true;
					})
					.test('min', `\${path} must be greater than 1 ${coinSymbol}`, (amount?: BigNumber) =>
						amount ? amount.shiftedBy(decimals).gte(minimumStake.toString()) : false,
					)
					.test('max', (amount: BigNumber | undefined, ctx) => {
						const gasBudget = ctx.parent.gasBudget || 0n;
						const availableBalance = coinBalance - gasBudget;
						if (availableBalance < 0) {
							return ctx.createError({
								message: 'Insufficient funds',
							});
						}
						const enoughBalance = amount
							? amount.shiftedBy(decimals).lte(availableBalance.toString())
							: false;
						if (enoughBalance) {
							return true;
						}
						return ctx.createError({
							message: `\${path} must be less than ${formatBalance(
								availableBalance,
								decimals,
							)} ${coinSymbol}`,
						});
					})
					.test(
						'max-decimals',
						`The value exceeds the maximum decimals (${decimals}).`,
						(amount?: BigNumber) => {
							return amount ? amount.shiftedBy(decimals).isInteger() : false;
						},
					)
					.label('Amount'),
	});
}
