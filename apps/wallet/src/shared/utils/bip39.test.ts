// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';

import { normalizeMnemonics, validateMnemonics } from './bip39';

describe('mnemonics', () => {
    it('normalize mnemonics', () => {
        expect(normalizeMnemonics(' Almost a Seed    Phrase')).toEqual(
            'almost a seed phrase'
        );
    });

    it('valid mnemonics', () => {
        expect(validateMnemonics('result')).toBe(false);
        expect(validateMnemonics('aaa')).toBe(false);
        expect(
            validateMnemonics(
                'random crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss'
            )
        ).toBe(false);

        expect(
            validateMnemonics(
                'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss'
            )
        ).toBeTruthy();
    });
});
