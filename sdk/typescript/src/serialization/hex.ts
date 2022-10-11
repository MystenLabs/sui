// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Buffer } from 'buffer';

export class HexDataBuffer {
  private _data: Uint8Array;

  constructor(data: Uint8Array | string) {
    if (typeof data === 'string') {
      this._data = new Uint8Array(Buffer.from(data, 'hex'));
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
    return Buffer.from(this._data).toString('hex');
  }
}
