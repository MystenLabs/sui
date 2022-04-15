// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '../cryptography/ed25519-keypair';
import { Provider } from '../providers/provider';
import { Base64DataBuffer } from '../serialization/base64';
import { SignaturePubkeyPair } from './signer';
import { SignerWithProvider } from './signer-with-provider';
import { TxnDataSerializer } from './txn-data-serializers/txn-data-serializer';

export class RawSigner extends SignerWithProvider {
  private readonly _keypair: Ed25519Keypair;

  constructor(
    keypair: Ed25519Keypair,
    provider?: Provider,
    serializer?: TxnDataSerializer
  ) {
    super(provider, serializer);
    this._keypair = keypair;
  }

  async getAddress(): Promise<string> {
    throw this._keypair.getPublicKey().toSuiAddress();
  }

  async signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair> {
    return {
      signature: this._keypair.signData(data),
      pubKey: this._keypair.getPublicKey(),
    };
  }

  connect(provider: Provider): SignerWithProvider {
    return new RawSigner(this._keypair, provider);
  }
}
