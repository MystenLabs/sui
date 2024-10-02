// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';

import { isValidSuiNSName, normalizeSuiNSName } from '../../../src/utils';

describe('isValidSuiNSName', () => {
	test('valid SuiNS names', () => {
		expect(isValidSuiNSName('example.sui')).toBe(true);
		expect(isValidSuiNSName('EXAMPLE.sui')).toBe(true);
		expect(isValidSuiNSName('@example')).toBe(true);
		expect(isValidSuiNSName('1.example.sui')).toBe(true);
		expect(isValidSuiNSName('1@example')).toBe(true);
		expect(isValidSuiNSName('a.b.c.example.sui')).toBe(true);
		expect(isValidSuiNSName('A.B.c.123@Example')).toBe(true);
		expect(isValidSuiNSName('1-a@1-b')).toBe(true);
		expect(isValidSuiNSName('1-a.1-b.sui')).toBe(true);
		expect(isValidSuiNSName('-@test')).toBe(false);
		expect(isValidSuiNSName('-1@test')).toBe(false);
		expect(isValidSuiNSName('test@-')).toBe(false);
		expect(isValidSuiNSName('test@-1')).toBe(false);
		expect(isValidSuiNSName('test@-a')).toBe(false);
		expect(isValidSuiNSName('test.sui2')).toBe(false);
		expect(isValidSuiNSName('.sui2')).toBe(false);
		expect(isValidSuiNSName('test@')).toBe(false);
		expect(isValidSuiNSName('@@')).toBe(false);
		expect(isValidSuiNSName('@@test')).toBe(false);
		expect(isValidSuiNSName('test@test.test')).toBe(false);
		expect(isValidSuiNSName('@test.test')).toBe(false);
		expect(isValidSuiNSName('#@test')).toBe(false);
		expect(isValidSuiNSName('test@#')).toBe(false);
		expect(isValidSuiNSName('test.#.sui')).toBe(false);
		expect(isValidSuiNSName('#.sui')).toBe(false);
		expect(isValidSuiNSName('@.test.sue')).toBe(false);

		expect(isValidSuiNSName('hello-.sui')).toBe(false);
		expect(isValidSuiNSName('hello--.sui')).toBe(false);
		expect(isValidSuiNSName('hello.-sui')).toBe(false);
		expect(isValidSuiNSName('hello.--sui')).toBe(false);
		expect(isValidSuiNSName('hello.sui-')).toBe(false);
		expect(isValidSuiNSName('hello.sui--')).toBe(false);
		expect(isValidSuiNSName('hello-@sui')).toBe(false);
		expect(isValidSuiNSName('hello--@sui')).toBe(false);
		expect(isValidSuiNSName('hello@-sui')).toBe(false);
		expect(isValidSuiNSName('hello@--sui')).toBe(false);
		expect(isValidSuiNSName('hello@sui-')).toBe(false);
		expect(isValidSuiNSName('hello@sui--')).toBe(false);
		expect(isValidSuiNSName('hello--world@sui')).toBe(false);
	});
});

describe('normalizeSuiNSName', () => {
	test('normalize SuiNS names', () => {
		expect(normalizeSuiNSName('example.sui')).toMatch('@example');
		expect(normalizeSuiNSName('EXAMPLE.sui')).toMatch('@example');
		expect(normalizeSuiNSName('@example')).toMatch('@example');
		expect(normalizeSuiNSName('1.example.sui')).toMatch('1@example');
		expect(normalizeSuiNSName('1@example')).toMatch('1@example');
		expect(normalizeSuiNSName('a.b.c.example.sui')).toMatch('a.b.c@example');
		expect(normalizeSuiNSName('A.B.c.123@Example')).toMatch('a.b.c.123@example');
		expect(normalizeSuiNSName('1-a@1-b')).toMatch('1-a@1-b');
		expect(normalizeSuiNSName('1-a.1-b.sui')).toMatch('1-a@1-b');

		expect(normalizeSuiNSName('example.sui', 'dot')).toMatch('example.sui');
		expect(normalizeSuiNSName('EXAMPLE.sui', 'dot')).toMatch('example.sui');
		expect(normalizeSuiNSName('@example', 'dot')).toMatch('example.sui');
		expect(normalizeSuiNSName('1.example.sui', 'dot')).toMatch('1.example.sui');
		expect(normalizeSuiNSName('1@example', 'dot')).toMatch('1.example.sui');
		expect(normalizeSuiNSName('a.b.c.example.sui', 'dot')).toMatch('a.b.c.example.sui');
		expect(normalizeSuiNSName('A.B.c.123@Example', 'dot')).toMatch('a.b.c.123.example.sui');
		expect(normalizeSuiNSName('1-a@1-b', 'dot')).toMatch('1-a.1-b.sui');
		expect(normalizeSuiNSName('1-a.1-b.sui', 'dot')).toMatch('1-a.1-b.sui');

		expect(() => normalizeSuiNSName('-@test')).toThrowError('Invalid SuiNS name -@test');
		expect(normalizeSuiNSName('1-a@1-b')).toMatchInlineSnapshot('"1-a@1-b"');
		expect(normalizeSuiNSName('1-a.1-b.sui')).toMatchInlineSnapshot('"1-a@1-b"');
		expect(() => normalizeSuiNSName('-@test')).toThrowError('Invalid SuiNS name -@test');
		expect(() => normalizeSuiNSName('-1@test')).toThrowError('Invalid SuiNS name -1@test');
		expect(() => normalizeSuiNSName('test@-')).toThrowError('Invalid SuiNS name test@-');
		expect(() => normalizeSuiNSName('test@-1')).toThrowError('Invalid SuiNS name test@-1');
		expect(() => normalizeSuiNSName('test@-a')).toThrowError('Invalid SuiNS name test@-a');
		expect(() => normalizeSuiNSName('test.sui2')).toThrowError('Invalid SuiNS name test.sui2');
		expect(() => normalizeSuiNSName('.sui2')).toThrowError('Invalid SuiNS name .sui2');
		expect(() => normalizeSuiNSName('test@')).toThrowError('Invalid SuiNS name test@');
		expect(() => normalizeSuiNSName('@@')).toThrowError('Invalid SuiNS name @@');
		expect(() => normalizeSuiNSName('@@test')).toThrowError('Invalid SuiNS name @@test');
		expect(() => normalizeSuiNSName('test@test.test')).toThrowError(
			'Invalid SuiNS name test@test.test',
		);
		expect(() => normalizeSuiNSName('@test.test')).toThrowError('Invalid SuiNS name @test.test');
		expect(() => normalizeSuiNSName('#@test')).toThrowError('Invalid SuiNS name #@test');
		expect(() => normalizeSuiNSName('test@#')).toThrowError('Invalid SuiNS name test@#');
		expect(() => normalizeSuiNSName('test.#.sui')).toThrowError('Invalid SuiNS name test.#.sui');
		expect(() => normalizeSuiNSName('#.sui')).toThrowError('Invalid SuiNS name #.sui');
		expect(() => normalizeSuiNSName('@.test.sue')).toThrowError('Invalid SuiNS name @.test.sue');
	});
});
