// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { normalizeMnemonics, validateMnemonics } from './bip39';

describe('mnemonics', () => {
	it('normalize mnemonics', () => {
		expect(normalizeMnemonics(' Almost a Seed    Phrase')).toEqual('almost a seed phrase');
	});

	it('validate mnemonics', () => {
		// Mnemonics length too short
		expect(validateMnemonics('result')).toBe(false);

		// Invalid word not from the wordlist
		expect(
			validateMnemonics(
				'aaa crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss',
			),
		).toBe(false);

		// Invalid checksum
		expect(
			validateMnemonics(
				'sleep kitten sleep kitten sleep kitten sleep kitten sleep kitten sleep kitten',
			),
		).toBe(false);

		// Test cases generated from https://iancoleman.io/bip39/ and https://github.com/trezor/python-mnemonic/blob/master/vectors.json
		// Valid mnemonics 12 words
		expect(
			validateMnemonics(
				'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss',
			),
		).toBeTruthy();

		// Valid mnemonics 15 words
		expect(
			validateMnemonics(
				'slender myth trap industry peanut arrange depart guess chef common steel rookie brick enroll napkin',
			),
		).toBeTruthy();

		// Valid mnemonics 18 words
		expect(
			validateMnemonics(
				'scissors invite lock maple supreme raw rapid void congress muscle digital elegant little brisk hair mango congress clump',
			),
		).toBeTruthy();

		// Valid mnemonics 21 words
		expect(
			validateMnemonics(
				'entry spoon private ridge clean salon loan surround apology fluid damage orbit embark digital polar find lazy bean plate burger august',
			),
		).toBeTruthy();

		// Valid mnemonics 24 words
		expect(
			validateMnemonics(
				'void come effort suffer camp survey warrior heavy shoot primary clutch crush open amazing screen patrol group space point ten exist slush involve unfold',
			),
		).toBeTruthy();

		// Mnemonics length too long
		expect(
			validateMnemonics(
				'abandon void come effort suffer camp survey warrior heavy shoot primary clutch crush open amazing screen patrol group space point ten exist slush involve unfold',
			),
		).toBe(false);
	});
});
