// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import nacl from 'tweetnacl';
import { Base64DataBuffer, Ed25519Keypair, RawSigner } from '../../src';
import { TextEncoder } from 'util';

describe('RawSigner', () => {
  it('signData', async () => {
    const keypair = new Ed25519Keypair();
    const signData = new Base64DataBuffer(
      new TextEncoder().encode('hello world')
    );
    const signer = new RawSigner(keypair);
    const { signature, pubKey } = await signer.signData(signData);
    const isValid = nacl.sign.detached.verify(
      signData.getData(),
      signature.getData(),
      pubKey.toBytes()
    );
    expect(isValid).toBeTruthy();
  });
});
