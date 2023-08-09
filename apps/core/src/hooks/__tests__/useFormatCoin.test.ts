// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';
import { describe, it, expect } from 'vitest';

import { formatBalance, CoinFormat } from '../useFormatCoin';

const SUI_DECIMALS = 9;

function toMist(sui: string) {
	return new BigNumber(sui).shiftedBy(SUI_DECIMALS).toString();
}

describe('formatBalance', () => {
	it('formats zero amounts correctly', () => {
		expect(formatBalance('0', 0)).toEqual('0');
		expect(formatBalance('0', SUI_DECIMALS)).toEqual('0');
	});

	it('formats decimal amounts correctly', () => {
		expect(formatBalance('0', SUI_DECIMALS)).toEqual('0');
		expect(formatBalance('0.000', SUI_DECIMALS)).toEqual('0');
	});

	it('formats integer amounts correctly', () => {
		expect(formatBalance(toMist('1'), SUI_DECIMALS)).toEqual('1');
		expect(formatBalance(toMist('1.0001'), SUI_DECIMALS)).toEqual('1');
		expect(formatBalance(toMist('1.1201'), SUI_DECIMALS)).toEqual('1.12');
		expect(formatBalance(toMist('1.1234'), SUI_DECIMALS)).toEqual('1.12');
		expect(formatBalance(toMist('1.1239'), SUI_DECIMALS)).toEqual('1.12');

		expect(formatBalance(toMist('9999.9999'), SUI_DECIMALS)).toEqual('9,999.99');
		// 10k + handling:
		expect(formatBalance(toMist('10000'), SUI_DECIMALS)).toEqual('10 K');
		expect(formatBalance(toMist('12345'), SUI_DECIMALS)).toEqual('12.34 K');
		// Millions:
		expect(formatBalance(toMist('1234000'), SUI_DECIMALS)).toEqual('1.23 M');
		// Billions:
		expect(formatBalance(toMist('1234000000'), SUI_DECIMALS)).toEqual('1.23 B');
	});

	it('formats integer amounts with full CoinFormat', () => {
		expect(formatBalance(toMist('1'), SUI_DECIMALS, CoinFormat.FULL)).toEqual('1');
		expect(formatBalance(toMist('1.123456789'), SUI_DECIMALS, CoinFormat.FULL)).toEqual(
			'1.123456789',
		);
		expect(formatBalance(toMist('9999.9999'), SUI_DECIMALS, CoinFormat.FULL)).toEqual('9,999.9999');
		expect(formatBalance(toMist('10000'), SUI_DECIMALS, CoinFormat.FULL)).toEqual('10,000');
		expect(formatBalance(toMist('12345'), SUI_DECIMALS, CoinFormat.FULL)).toEqual('12,345');
		expect(formatBalance(toMist('1234000'), SUI_DECIMALS, CoinFormat.FULL)).toEqual('1,234,000');
		expect(formatBalance(toMist('1234000000'), SUI_DECIMALS, CoinFormat.FULL)).toEqual(
			'1,234,000,000',
		);
	});
});
