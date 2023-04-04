// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { convertCommandArgumentToString } from './Command';

describe('convertCommandArgumentToString', () => {
    it('should convert a null when argument is undefined', () => {
        expect(convertCommandArgumentToString(undefined)).toBe(null);
    });

    it('should convert a string when argument is a string', () => {
        expect(convertCommandArgumentToString('test')).toBe('test');
    });

    it('should convert a [Bytes] when argument is a Number[][]', () => {
        expect(convertCommandArgumentToString([[1, 2, 3]])).toBe('[Bytes]');
    });

    it('should convert a null when argument is None in argument', () => {
        expect(
            convertCommandArgumentToString({
                None: null,
            })
        ).toBe(null);
    });

    it('should convert a arg.Some when argument is a Some in argument', () => {
        expect(
            convertCommandArgumentToString({
                Some: 'arg.Some',
            })
        ).toBe('arg.Some');
    });

    it('should convert a string when argument is a string[] with multiple elements', () => {
        expect(convertCommandArgumentToString(['test', 'test2'])).toBe(
            '[test, test2]'
        );
    });

    it('should convert a GasCoin when argument is a GasCoin in argument', () => {
        expect(
            convertCommandArgumentToString({
                kind: 'GasCoin',
            })
        ).toBe('GasCoin');
    });

    it('should convert a Input when argument is a Input in argument', () => {
        expect(
            convertCommandArgumentToString({
                kind: 'Input',
                value: 'test',
                index: 0,
            })
        ).toBe('Input(0)');
    });

    it('should convert a Result when argument is a Result in argument', () => {
        expect(
            convertCommandArgumentToString({
                kind: 'Result',
                index: 0,
            })
        ).toBe('Result(0)');
    });

    it('should convert a NestedResult when argument is a NestedResult in argument', () => {
        expect(
            convertCommandArgumentToString({
                kind: 'NestedResult',
                index: 0,
                resultIndex: 1,
            })
        ).toBe('NestedResult(0, 1)');
    });
});
