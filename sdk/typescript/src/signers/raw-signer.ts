// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Keypair } from '../cryptography/keypair';
import { Provider } from '../providers/provider';
import { Base64DataBuffer } from '../serialization/base64';
import { SuiAddress } from '../types';
import { SignaturePubkeyPair } from './signer';
import { SignerWithProvider } from './signer-with-provider';
import { TxnDataSerializer } from './txn-data-serializers/txn-data-serializer';

export class RawSigner extends SignerWithProvider {
  private readonly keypair: Keypair;

  constructor(
    keypair: Keypair,
    provider?: Provider,
    serializer?: TxnDataSerializer
  ) {
    super(provider, serializer);
    this.keypair = keypair;
  }

  async getAddress(): Promise<SuiAddress> {
    return this.keypair.getPublicKey().toSuiAddress();
  }

  async signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair> {
    return {
      signatureScheme: this.keypair.getKeyScheme(),
      signature: this.keypair.signData(data),
      pubKey: this.keypair.getPublicKey(),
    };
  }

  connect(provider: Provider): SignerWithProvider {
    return new RawSigner(this.keypair, provider);
  }
}
