// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';

export class Base64DataBuffer {
  private data: Uint8Array;

  constructor(data: Uint8Array | string) {
    if (typeof data === 'string') {
      this.data = fromB64(data);
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
    return toB64(this.data);
  }
}
