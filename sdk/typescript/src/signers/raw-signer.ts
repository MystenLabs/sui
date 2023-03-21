// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { blake2b } from '@noble/hashes/blake2b';
import { Keypair } from '../cryptography/keypair';
import {
  SerializedSignature,
  toSerializedSignature,
} from '../cryptography/signature';
import { JsonRpcProvider } from '../providers/json-rpc-provider';
import { SuiAddress } from '../types';
import { SignerWithProvider } from './signer-with-provider';

export class RawSigner extends SignerWithProvider {
  private readonly keypair: Keypair;

  constructor(keypair: Keypair, provider: JsonRpcProvider) {
    super(provider);
    this.keypair = keypair;
  }

  async getAddress(): Promise<SuiAddress> {
    return this.keypair.getPublicKey().toSuiAddress();
  }

  async signData(data: Uint8Array): Promise<SerializedSignature> {
    const pubkey = this.keypair.getPublicKey();
    const digest = blake2b(data, { dkLen: 32 });
    const signature = this.keypair.signData(digest, false);
    const signatureScheme = this.keypair.getKeyScheme();

    return toSerializedSignature({
      signatureScheme,
      signature,
      pubKey: pubkey,
    });
  }

  connect(provider: JsonRpcProvider): SignerWithProvider {
    return new RawSigner(this.keypair, provider);
  }
}
