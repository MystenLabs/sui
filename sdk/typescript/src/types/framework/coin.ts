// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectContent } from '../objects';
import { MoveVersionedID } from './id';
import { MoveObjectContent, MoveObjectType } from './move-object';

/**
 * Typescript version for Sui::Coin module
 * as defined in https://github.com/MystenLabs/sui/blob/ca9046fd8b1a9e8634a4b74b0e7dabdc7ea54475/sui_programmability/framework/sources/Coin.move#L4
 */
export class Coin extends MoveObjectContent {
  static isInstance(data: ObjectContent): boolean {
    return (
      MoveObjectContent.parseType(data).getWithoutGeneric() == '0x2::Coin::Coin'
    );
  }

  amount(): number {
    return this.data.fields['value'] as number;
  }

  symbol(): string {
    return this.getCoinType()
      .getStructName()
      .getStructName();
  }

  id(): MoveVersionedID {
    return new MoveVersionedID(this.data.fields['id'] as ObjectContent);
  }

  private getCoinType(): MoveObjectType {
    return this.getType().getGenericType()!;
  }

  toJSON(): string {
    return JSON.stringify({
      amount: this.amount(),
      symbol: this.symbol(),
      versioned_id: this.id().toJSON(),
    });
  }
}
