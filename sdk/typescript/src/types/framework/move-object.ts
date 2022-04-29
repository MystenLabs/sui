// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isObjectContent } from '../../index.guard';
import { ObjectContent } from '../objects';

export class MoveStructName {
  constructor(public data: string) {}

  getFullType(): string {
    return this.data;
  }

  isNested(): boolean {
    return (this.data.match(/</g) || []).length > 1;
  }

  getRawStructName(): string {
    return this.data;
  }

  getStructName(): string {
    return this.data.split('<')[0];
  }

  // TODO: handle multiple generic types
  getGenericType(): MoveObjectType | null {
    if (!this.hasGenericType()) {
      return null;
    }

    const generic = this.data.match(/^\w+<(.*)>$/)![1];
    return new MoveObjectType(generic);
  }

  hasGenericType(): boolean {
    return this.data.includes('<');
  }
}

export class MoveObjectType {
  constructor(public data: string) {}

  getFullType(): string {
    return this.data;
  }

  getPackageAddress(): string {
    return this.data.split('::')[0];
  }

  getModuleName(): string {
    return this.data.split('::')[1];
  }

  getStructName(): MoveStructName {
    const re = /^\w+::\w+::(.+)$/;
    const found = this.data.match(re)![1];
    return new MoveStructName(found);
  }

  hasGenericType(): boolean {
    return this.getStructName().hasGenericType();
  }

  getWithoutGeneric(): string {
    return this.data.split('<')[0];
  }

  getGenericType(): MoveObjectType | null {
    return this.getStructName().getGenericType();
  }

  toString() {
    return this.data;
  }
}

export abstract class MoveObjectContent {
  // static methods
  // Needs override
  static isInstance(_data: ObjectContent): boolean {
    return false;
  }

  static parseType(data: ObjectContent): MoveObjectType {
    return new MoveObjectType(data.type);
  }

  // instance methods
  constructor(public data: ObjectContent) {}

  getType(): MoveObjectType {
    return new MoveObjectType(this.data.type);
  }

  toJSON(): string {
    return MoveObjectContent.parseJSON(this.data);
  }

  toString(): string {
    return this.toJSON();
  }

  static parseJSON(data: ObjectContent): string {
    let obj: Record<string, string> = {};
    Object.entries<any>(data.fields).forEach(([key, value]) => {
      if (isObjectContent(value)) {
        obj[key] = this.parseJSON(value) || '';
      } else {
        obj[key] = value;
      }
    });
    return JSON.stringify(obj);
  }
}
