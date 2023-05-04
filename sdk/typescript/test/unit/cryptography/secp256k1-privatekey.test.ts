// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64, toHEX } from '@mysten/bcs';
import { describe, it, expect } from 'vitest';
import { Secp256k1PublicKey } from '../../../src/cryptography/secp256k1-publickey';
import {
  INVALID_SECP256K1_PUBLIC_KEY,
  VALID_SECP256K1_PUBLIC_KEY,
} from './secp256k1-keypair.test';
import { Secp256k1Keypair } from '../../../src';
import { bytesToHex } from '@noble/hashes/utils';

// Test cases generated on the fly;
const TEST_CASES: [string, string, string][] = [];
for (let i = 0; i < 5; i++) {
  const account = Secp256k1Keypair.generate();
  const secret = `0x${bytesToHex(account.getSecretKey())}`;

  TEST_CASES.push([
    account.getPublicKey().toSuiAddress(),
    secret,
    account.export().privateKey,
  ]);
}

describe('Secp256k1PublicKey', () => {
  TEST_CASES.forEach(([suiAddress, suiPrivateKey, privateKey]) => {
    it(`getSecretKey for address ${suiAddress}`, () => {
      const raw = fromB64(privateKey);
      const account = Secp256k1Keypair.fromSecretKey(raw);
      expect(toB64(account.getSecretKey())).toEqual(privateKey);
    });

    it(`exportToSui().privateKey for address ${suiAddress}`, () => {
      const raw = fromB64(privateKey);
      const account = Secp256k1Keypair.fromSecretKey(raw);
      expect(account.exportToSui().suiPrivateKey).toEqual(suiPrivateKey);
      expect(account.exportToSui().suiAddress).toEqual(suiAddress);
    });

    it(`fromSuiSecretKey for address ${suiAddress}`, async function () {
      const account = Secp256k1Keypair.fromSuiSecretKey(suiPrivateKey);

      expect(account.exportToSui().suiPrivateKey).toEqual(suiPrivateKey);
      expect(account.exportToSui().suiAddress).toEqual(suiAddress);
      expect(account.export().privateKey).toEqual(privateKey);
    });
  });
});
