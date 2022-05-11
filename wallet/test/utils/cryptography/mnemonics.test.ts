// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer, Ed25519Keypair } from '@mysten/sui.js';

import {
    generateMnemonicsAndKeypair,
    getKeypairFromMnemonics,
    normalizeMnemonics,
} from '../../../src/utils/cryptography/mnemonics';

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
});
