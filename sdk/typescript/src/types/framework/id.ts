// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectContent } from '../objects';
import { MoveObjectContent } from './move-object';

/**
 * Typescript version for Sui::ID Module
 * as defined in https://github.com/MystenLabs/sui/blob/ca9046fd8b1a9e8634a4b74b0e7dabdc7ea54475/sui_programmability/framework/sources/ID.move#L48
 */
export class MoveID extends MoveObjectContent {
  static isInstance(data: ObjectContent): boolean {
    return MoveObjectContent.parseType(data).getFullType() == '0x2::ID::ID';
  }

  id(): string {
    return this.data.fields['bytes'] as string;
  }

  toJSON() {
    return JSON.stringify({
      id: this.id(),
    });
  }
}

export class MoveUniqueID extends MoveObjectContent {
  static isInstance(data: ObjectContent): boolean {
    return (
      MoveObjectContent.parseType(data).getFullType() == '0x2::ID::UniqueID'
    );
  }

  id(): MoveID {
    return new MoveID(this.data.fields['id'] as ObjectContent);
  }

  toJSON() {
    return this.id().toJSON();
  }
}

export class MoveVersionedID extends MoveObjectContent {
  static isInstance(data: ObjectContent): boolean {
    return (
      MoveObjectContent.parseType(data).getFullType() == '0x2::ID::VersionedID'
    );
  }

  typedID(): MoveUniqueID {
    return new MoveUniqueID(this.data.fields['id'] as ObjectContent);
  }

  id(): string {
    return new MoveUniqueID(this.data.fields['id'] as ObjectContent).id().id();
  }

  version(): number {
    return this.data.fields['version'] as number;
  }

  toJSON() {
    return JSON.stringify({
      id: this.id(),
      version: this.version(),
    });
  }
}
