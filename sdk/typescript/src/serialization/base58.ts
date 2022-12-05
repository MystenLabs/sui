// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import bs58 from 'bs58';

export class Base58DataBuffer {
  private data: Uint8Array;

  constructor(data: Uint8Array | string) {
    if (typeof data === 'string') {
      this.data = bs58.decode(data);
    } else {
      this.data = data;
    }
  }

  getData(): Uint8Array {
    return this.data;
  }

  getLength(): number {
    return this.data.length;
  }

  toString(): string {
    return bs58.encode(this.data);
  }
}
