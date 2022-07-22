// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import nacl from 'tweetnacl';
import { Base64DataBuffer, Ed25519Keypair } from '../../src';
import { TextEncoder } from 'util';

const VALID_SECRET_KEY =
  'mdqVWeFekT7pqy5T49+tV12jO0m+ESW7ki4zSU9JiCgbL0kJbj5dvQ/PqcDAzZLZqzshVEs01d1KZdmLh4uZIg==';
const INVALID_SECRET_KEY =
  'mdqVWeFekT7pqy5T49+tV12jO0m+ESW7ki4zSU9JiCgbL0kJbj5dvQ/PqcDAzZLZqzshVEs01d1KZdmLh4uZIG==';

describe('ed25519-keypair', () => {
  it('new keypair', () => {
    const keypair = new Ed25519Keypair();
    expect(keypair.getPublicKey().toBytes().length).toBe(32);
    expect(2).toEqual(2);
  });

  it('create keypair from secret key', () => {
    const secretKey = Buffer.from(VALID_SECRET_KEY, 'base64');
    const keypair = Ed25519Keypair.fromSecretKey(secretKey);
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'Gy9JCW4+Xb0Pz6nAwM2S2as7IVRLNNXdSmXZi4eLmSI='
    );
  });

  it('creating keypair from invalid secret key throws error', () => {
    const secretKey = Buffer.from(INVALID_SECRET_KEY, 'base64');
    expect(() => {
      Ed25519Keypair.fromSecretKey(secretKey);
    }).toThrow('provided secretKey is invalid');
  });

  it('creating keypair from invalid secret key succeeds if validation is skipped', () => {
    const secretKey = Buffer.from(INVALID_SECRET_KEY, 'base64');
    const keypair = Ed25519Keypair.fromSecretKey(secretKey, {
      skipValidation: true,
    });
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'Gy9JCW4+Xb0Pz6nAwM2S2as7IVRLNNXdSmXZi4eLmSA='
    );
  });

  it('generate keypair from random seed', () => {
    const keypair = Ed25519Keypair.fromSeed(Uint8Array.from(Array(32).fill(8)));
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'E5j2LG0aRXxRumpLXz29L2n8qTIWIY3ImX5Ba9F9k8o='
    );
  });

  it('signature of data is valid', () => {
    const keypair = new Ed25519Keypair();
    const signData = new Base64DataBuffer(
      new TextEncoder().encode('hello world')
    );
    const signature = keypair.signData(signData);
    const isValid = nacl.sign.detached.verify(
      signData.getData(),
      signature.getData(),
      keypair.getPublicKey().toBytes()
    );
    expect(isValid).toBeTruthy();
  });
});
