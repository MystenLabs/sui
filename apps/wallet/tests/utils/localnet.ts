// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// eslint-disable-next-line import/order
import 'tsconfig-paths/register';
import { Ed25519Keypair } from '@mysten/sui.js';
import * as bip39 from '@scure/bip39';
import { wordlist } from '@scure/bip39/wordlists/english';

export async function generateKeypair() {
    const mnemonic = bip39.generateMnemonic(wordlist);
    const keypair = Ed25519Keypair.deriveKeypair(mnemonic);
    return { mnemonic, keypair };
}
