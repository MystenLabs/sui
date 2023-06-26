// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { flattenSuiArguments } from '~/pages/transaction-result/programmable-transaction-view/utils';

describe('utils.ts', () => {
	describe('flattenCommandData', () => {
		it('should format SplitCoin data', () => {
			expect(flattenSuiArguments(['GasCoin', { Input: 1 }])).toEqual('GasCoin, Input(1)');
			expect(flattenSuiArguments(['GasCoin', { Result: 2 }])).toEqual('GasCoin, Result(2)');
			expect(flattenSuiArguments(['GasCoin', { NestedResult: [1, 2] }])).toEqual(
				'GasCoin, NestedResult(1, 2)',
			);
		});
		it('should format TransferObjects data', () => {
			expect(
				flattenSuiArguments([
					[
						{
							Result: 0,
						},
						{
							Result: 1,
						},
						{
							Result: 2,
						},
						{
							Result: 3,
						},
						{
							Result: 4,
						},
					],
					{
						Input: 0,
					},
				]),
			).toEqual('[Result(0), Result(1), Result(2), Result(3), Result(4)], Input(0)');
		});
		it('should flatten MergeCoinsSuiTransaction data', () => {
			expect(
				flattenSuiArguments([
					{
						Input: 0,
					},
					[
						{
							Result: 0,
						},
						{
							Result: 1,
						},
						{
							Result: 2,
						},
						{
							Result: 3,
						},
						{
							Result: 4,
						},
					],
				]),
			).toEqual('Input(0), [Result(0), Result(1), Result(2), Result(3), Result(4)]');
		});
	});
});
