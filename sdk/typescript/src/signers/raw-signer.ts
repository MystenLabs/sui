// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';
import { gt } from '@suchipi/femver';
import { Keypair } from '../cryptography/keypair';
import { SIGNATURE_SCHEME_TO_FLAG } from '../cryptography/publickey';
import { Provider } from '../providers/provider';
import { SuiAddress, versionToString } from '../types';
import { messageWithIntent } from '../utils/intent';
import { SignaturePubkeyPair } from './signer';
import { SignerWithProvider } from './signer-with-provider';
import { TxnDataSerializer } from './txn-data-serializers/txn-data-serializer';

export class RawSigner extends SignerWithProvider {
  private readonly keypair: Keypair;

  constructor(
    keypair: Keypair,
    provider?: Provider,
    serializer?: TxnDataSerializer,
  ) {
    super(provider, serializer);
    this.keypair = keypair;
  }

  async getAddress(): Promise<SuiAddress> {
    return this.keypair.getPublicKey().toSuiAddress();
  }

  async signData(data: Uint8Array): Promise<SignaturePubkeyPair> {
    // Starting Sui 0.25.0, only 64-byte nonrecoverable signatures are accepted.
    // TODO(joyqvq): Remove once 0.25.0 is released.
    const version = await this.provider.getRpcApiVersion();
    let useRecoverable =
      version && gt(versionToString(version), '0.24.0') ? false : true;

    const pubkey = this.keypair.getPublicKey();
    const signature = this.keypair.signData(data, useRecoverable);
    const signatureScheme = this.keypair.getKeyScheme();

    const serialized_sig = new Uint8Array(
      1 + signature.length + pubkey.toBytes().length,
    );
    serialized_sig.set([SIGNATURE_SCHEME_TO_FLAG[signatureScheme]]);
    serialized_sig.set(signature, 1);
    serialized_sig.set(pubkey.toBytes(), 1 + signature.length);

    return {
      signatureScheme,
      signature: toB64(signature),
      pubKey: pubkey.toBase64(),
      serializedSignature: toB64(serialized_sig),
    };
  }

  connect(provider: Provider): SignerWithProvider {
    return new RawSigner(this.keypair, provider);
  }
}
