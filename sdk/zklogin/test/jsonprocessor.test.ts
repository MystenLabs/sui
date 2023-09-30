// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { describe, expect, it } from 'vitest';

import { JSONProcessor } from '../src/jsonprocessor';

describe('JSONProcessor', () => {
	const jwt =
		'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c';
	const decoded_payload = Buffer.from(jwt.split('.')[1], 'base64url').toString();

	describe('constructor', () => {
		it('should initialize the events array', () => {
			const processor = new JSONProcessor(decoded_payload);
			expect(processor.events).toBeDefined();
			expect(processor.events.length).toBeGreaterThan(0);
		});

		it('should initialize the processed object', () => {
			const processor = new JSONProcessor(decoded_payload);
			expect(processor.processed).toBeDefined();
			expect(Object.keys(processor.processed).length).toBe(0);
		});
	});

	describe('process', () => {
		it('should throw an error if the claim is not found', () => {
			const processor = new JSONProcessor(decoded_payload);
			expect(() => processor.process('invalid_claim')).toThrowError(
				'Claim invalid_claim not found',
			);
		});

		it('should throw an error if the JSON is invalid (colon is not found)', () => {
			const invalidJwt = '{"sub" "1234567890"}';
			expect(() => new JSONProcessor(invalidJwt)).toThrowError('Parse errors encountered');
		});

		it('should throw an error if the JSON is invalid (value not found)', () => {
			const invalidJwt = '{"sub":}';
			expect(() => new JSONProcessor(invalidJwt)).toThrowError('Parse errors encountered');
		});

		it('should throw an error if the JSON is invalid (extra comma)', () => {
			const invalidJwt = '{"sub":"1234567890",}';
			expect(() => new JSONProcessor(invalidJwt)).toThrowError('Parse errors encountered');
		});

		it('should throw an error if the JSON is not expected (value is an array)', () => {
			const invalidJwt = '{"aud":[1234567890, 123]}';
			const processor = new JSONProcessor(invalidJwt);
			expect(() => processor.process('aud')).toThrowError(
				'Unexpected JSON value type: onArrayBegin',
			);
		});

		it('should throw an error if the JSON is not expected (value is a number)', () => {
			const invalidJwt = '{"sub":1234567890}';
			const processor = new JSONProcessor(invalidJwt);
			expect(() => processor.process('sub')).toThrowError('Unexpected type for sub');
		});

		it('should throw an error if the JSON is not expected (value is a object)', () => {
			const invalidJwt = '{"sub":{"sub":1234567890}}';
			const processor = new JSONProcessor(invalidJwt);
			expect(() => processor.process('sub')).toThrowError(
				'Unexpected JSON value type: onObjectBegin',
			);
		});

		it('should process a JWT with a single claim', () => {
			const input = '{"sub":"12345"}';
			const processor = new JSONProcessor(input);
			const claim_details = processor.process('sub');
	
			expect(claim_details.name).toEqual('sub');
			expect(claim_details.value).toEqual('12345');
			expect(claim_details.ext_claim).toEqual('"sub":"12345"}');
			expect(claim_details.offsets.start).toEqual(1);
			expect(claim_details.offsets.colon).toEqual(5);
			expect(claim_details.offsets.value).toEqual(6);
			expect(claim_details.offsets.name_length).toEqual(5);
			expect(claim_details.offsets.value_length).toEqual(7);
			expect(claim_details.offsets.ext_length).toEqual('"sub":"12345"}'.length);
		});
	
		it('should return the claim details', () => {
			const processor = new JSONProcessor(decoded_payload);
			const claimDetails = processor.process('sub');
			expect(claimDetails).toBeDefined();
			expect(claimDetails.name).toBe('sub');
			expect(claimDetails.value).toBe('1234567890');
			expect(claimDetails.ext_claim).toBe('"sub":"1234567890",');
			expect(claimDetails.offsets.start).toBe(1);
			expect(claimDetails.offsets.colon).toBe(5);
			expect(claimDetails.offsets.value).toBe(6);
			expect(claimDetails.offsets.value_length).toBe(12);
			expect(claimDetails.offsets.name_length).toBe(5);
			expect(claimDetails.offsets.ext_length).toBe(19);
		});

		it('should process a JWT with whitespaces', () => {
			const input = '{ "sub" : "hello" }';
			const processor = new JSONProcessor(input);
			const claim_details = processor.process('sub');
	
			expect(claim_details.name).toEqual('sub');
			expect(claim_details.value).toEqual('hello');
			expect(claim_details.ext_claim).toEqual('"sub" : "hello" }');
			expect(claim_details.offsets.start).toEqual(2);
			expect(claim_details.offsets.colon).toEqual(6);
			expect(claim_details.offsets.value).toEqual(8);
			expect(claim_details.offsets.value_length).toEqual('"hello"'.length);
			expect(claim_details.offsets.ext_length).toEqual('"sub" : "hello" }'.length);
		});
	
		it('should cache the claim details', () => {
			const processor = new JSONProcessor(decoded_payload);
			const claimDetails1 = processor.process('sub');
			const claimDetails2 = processor.process('sub');
			expect(claimDetails1).toBe(claimDetails2);
		});
	});

	describe('getRawClaimValue', () => {
		it('should throw an error if the claim is not processed', () => {
			const processor = new JSONProcessor(decoded_payload);
			expect(() => processor.getRawClaimValue('invalid_claim')).toThrowError(
				'Claim invalid_claim not processed',
			);
		});

		it('should return the raw claim value', () => {
			const processor = new JSONProcessor(decoded_payload);
			processor.process('sub');
			const rawValue = processor.getRawClaimValue('sub');
			expect(rawValue).toBe('1234567890');
		});
	});
});
