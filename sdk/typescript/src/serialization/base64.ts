// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Buffer } from 'buffer';

export class Base64DataBuffer {
  private data: Uint8Array;

  constructor(data: Uint8Array | string) {
    if (typeof data === 'string') {
      this.data = new Uint8Array(Buffer.from(data, 'base64'));
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
    return Buffer.from(this.data).toString('base64');
  }
}
