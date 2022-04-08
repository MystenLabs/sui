// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Buffer } from 'buffer';

// TODO: Buffer is not supported in browser environments
export class Base64DataBuffer {
  private _data: Uint8Array;

  constructor(data: Uint8Array | string) {
    if (typeof data === 'string') {
      this._data = new Uint8Array(Buffer.from(data, 'base64'));
    } else {
      this._data = data;
    }
  }

  getData(): Uint8Array {
    return this._data;
  }

  getLength(): number {
    return this._data.length;
  }

  toString(): string {
    return Buffer.from(this._data).toString('base64');
  }
}
