// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { CoinStruct } from '@mysten/sui.js/client';

const COIN_TYPE = '0x2::coin::Coin';
const COIN_TYPE_ARG_REGEX = /^0x2::coin::Coin<(.+)>$/;

export const GAS_TYPE_ARG = '0x2::sui::SUI';
export const GAS_SYMBOL = 'SUI';

// TODO use sdk
export class Coin {
	public static isCoin(obj: CoinStruct) {
		const type = obj?.coinType === 'package' ? 'package' : obj?.coinType;
		return type?.startsWith(COIN_TYPE) ?? false;
	}

	public static getCoinTypeArg(obj: CoinStruct) {
		const res = obj.coinType.match(COIN_TYPE_ARG_REGEX);
		return res ? res[1] : null;
	}

	public static isSUI(obj: CoinStruct) {
		const arg = Coin.getCoinTypeArg(obj);
		return arg ? Coin.getCoinSymbol(arg) === 'SUI' : false;
	}

	public static getCoinSymbol(coinTypeArg: string) {
		return coinTypeArg.substring(coinTypeArg.lastIndexOf(':') + 1);
	}

	public static getBalance(obj: CoinStruct): bigint {
		return BigInt(obj.balance);
	}

	public static getID(obj: CoinStruct): string {
		return obj.coinObjectId;
	}

	public static getCoinTypeFromArg(coinTypeArg: string) {
		return `${COIN_TYPE}<${coinTypeArg}>`;
	}
}
