// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiMoveObject, SuiObjectData } from '@mysten/sui/client';

const COIN_TYPE = '0x2::coin::Coin';
const COIN_TYPE_ARG_REGEX = /^0x2::coin::Coin<(.+)>$/;

export const GAS_TYPE_ARG = '0x2::sui::SUI';
export const GAS_SYMBOL = 'SUI';

// TODO use sdk
export class Coin {
	public static isCoin(obj: SuiObjectData) {
		const type = obj?.content?.dataType === 'package' ? 'package' : obj?.content?.type;
		return type?.startsWith(COIN_TYPE) ?? false;
	}

	public static getCoinTypeArg(obj: SuiMoveObject) {
		const res = obj.type.match(COIN_TYPE_ARG_REGEX);
		return res ? res[1] : null;
	}

	public static isSUI(obj: SuiMoveObject) {
		const arg = Coin.getCoinTypeArg(obj);
		return arg ? Coin.getCoinSymbol(arg) === 'SUI' : false;
	}

	public static getCoinSymbol(coinTypeArg: string) {
		return coinTypeArg.substring(coinTypeArg.lastIndexOf(':') + 1);
	}

	public static getBalance(obj: SuiMoveObject): bigint {
		return BigInt((obj.fields as { balance: string }).balance);
	}

	public static getID(obj: SuiMoveObject): string {
		return (obj.fields as { id: { id: string } }).id.id;
	}

	public static getCoinTypeFromArg(coinTypeArg: string) {
		return `${COIN_TYPE}<${coinTypeArg}>`;
	}
}
