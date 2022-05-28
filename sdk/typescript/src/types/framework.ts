// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectFields, GetObjectDataResponse } from './objects';

import { getMoveObjectType } from './objects';

import BN from 'bn.js';

/**
 * Utility class for 0x2::Coin
 * as defined in https://github.com/MystenLabs/sui/blob/ca9046fd8b1a9e8634a4b74b0e7dabdc7ea54475/sui_programmability/framework/sources/Coin.move#L4
 */
export class Coin {
  static isCoin(data: GetObjectDataResponse): boolean {
    return getMoveObjectType(data)?.startsWith('0x2::Coin::Coin') ?? false;
  }

  static getBalance(data: GetObjectDataResponse): BN | undefined {
    if (!Coin.isCoin(data)) {
      return undefined;
    }
    const balance = getObjectFields(data)?.balance;
    return new BN.BN(balance, 10);
  }

  static getZero(): BN {
    return new BN.BN('0', 10);
  }
}
