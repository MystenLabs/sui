// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Base64DataBuffer,
    Ed25519Keypair,
    Secp256k1Keypair,
} from '@mysten/sui.js';
import { describe, it, expect } from 'vitest';

import {
    deriveKeypairFromMnemonics,
    deriveSecp256k1KeypairFromMnemonics,
    generateMnemonicsAndKeypair,
    getKeypairFromMnemonics,
    normalizeMnemonics,
} from './mnemonics';

// TODO(joyqvq): move this and its test file to sdk/typescript/cryptography
describe('mnemonics', () => {
    it('generate mnemonics', () => {
        const [mnemonics, keypair] = generateMnemonicsAndKeypair();
        expect(mnemonics.split(' ').length).toBe(12);
        const parsedKeypair = getKeypairFromMnemonics(mnemonics);
        expect(parsedKeypair.publicKey).toEqual(keypair.publicKey);
        expect(parsedKeypair.secretKey).toEqual(keypair.secretKey);
    });

    it('normalize', () => {
        expect(normalizeMnemonics(' Almost a Seed    Phrase')).toEqual(
            'almost a seed phrase'
        );
    });

    it('parse mnemonics', () => {
        const keypairData = getKeypairFromMnemonics(
            'Shoot island position soft burden budget tooth cruel issue economy destroy Above'
        );

        const keypair = new Ed25519Keypair(keypairData);

        expect(new Base64DataBuffer(keypairData.secretKey).toString()).toEqual(
            'V3zZEK7eJYJminQdR2tF55mOkFpChvcBuHslkjUB+dTGHsYX9tdbrbyu2sALKKQcfTOiz6DAeMOxQ+RNp159nA=='
        );
        expect(keypair.getPublicKey().toBase64()).toEqual(
            'xh7GF/bXW628rtrACyikHH0zos+gwHjDsUPkTadefZw='
        );
    });

    it('invalid mnemonics to get keypair', () => {
        expect(() => {
            getKeypairFromMnemonics('aaa');
        }).toThrow('Invalid mnemonics');
    });

    it('derive ed25519 keypair from path and mnemonics', () => {
        // Test case generated against rust: /sui/crates/sui/src/unit_tests/keytool_tests.rs#L149
        const keypairData = deriveKeypairFromMnemonics(
            `m/44'/784'/0'/0'/0'`,
            'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss'
        );
        const keypair = new Ed25519Keypair(keypairData);

        expect(keypair.getPublicKey().toBase64()).toEqual(
            'aFstb5h4TddjJJryHJL1iMob6AxAqYxVv3yRt05aweI='
        );
        expect(keypair.getPublicKey().toSuiAddress()).toEqual(
            '1a4623343cd42be47d67314fce0ad042f3c82685'
        );
    });

    it('incorrect coin type node for ed25519 derivation path', () => {
        expect(() => {
            deriveKeypairFromMnemonics(
                `m/44'/0'/0'/0'/0'`,
                'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss'
            );
        }).toThrow('Invalid derivation path');
    });

    it('incorrect purpose node for ed25519 derivation path', () => {
        expect(() => {
            deriveKeypairFromMnemonics(
                `m/54'/784'/0'/0'/0'`,
                'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss'
            );
        }).toThrow('Invalid derivation path');
    });

    it('invalid mnemonics to derive ed25519 keypair', () => {
        expect(() => {
            deriveKeypairFromMnemonics(`m/44'/784'/0'/0'/0'`, 'aaa');
        }).toThrow('Invalid mnemonics');
    });

    it('derive secp256k1 keypair from path and mnemonics', () => {
        // Test case generated against rust: /sui/crates/sui/src/unit_tests/keytool_tests.rs#L149
        const keypairData = deriveSecp256k1KeypairFromMnemonics(
            `m/54'/784'/0'/0/0`,
            'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss'
        );
        const keypair = new Secp256k1Keypair(keypairData);

        expect(keypair.getPublicKey().toBase64()).toEqual(
            'A+NxdDVYKrM9LjFdIem8ThlQCh/EyM3HOhU2WJF3SxMf'
        );
        expect(keypair.getPublicKey().toSuiAddress()).toEqual(
            'ed17b3f435c03ff69c2cdc6d394932e68375f20f'
        );
    });

    it('incorrect purpose node for secp256k1 derivation path', () => {
        expect(() => {
            deriveSecp256k1KeypairFromMnemonics(
                `m/44'/784'/0'/0'/0'`,
                'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss'
            );
        }).toThrow('Invalid derivation path');
    });

    it('incorrect hardened path for secp256k1 key derivation', () => {
        expect(() => {
            deriveSecp256k1KeypairFromMnemonics(
                `m/54'/784'/0'/0'/0'`,
                'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss'
            );
        }).toThrow('Invalid derivation path');
    });
});
