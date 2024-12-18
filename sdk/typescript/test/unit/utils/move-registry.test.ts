// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';

import { isValidNamedPackage, isValidNamedType } from '../../../src/utils';

describe('isValidNamedPackage', () => {
	test('Valid/Invalid .move names', () => {
		expect(isValidNamedPackage('@org/app')).toBe(true);
		expect(isValidNamedPackage('@org/app/1')).toBe(true);
		expect(isValidNamedPackage('@org/app/123')).toBe(true);
		expect(isValidNamedPackage('@org/app-test/1')).toBe(true);
		expect(isValidNamedPackage('@org/1app/1')).toBe(true);
		expect(isValidNamedPackage('@org/1-app/1')).toBe(true);
		expect(isValidNamedPackage('test@o-rg/1')).toBe(true);
		expect(isValidNamedPackage('org.sui/app')).toBe(true);
		expect(isValidNamedPackage('org.sui/app/1')).toBe(true);

		// failed scenarios.
		expect(isValidNamedPackage('@org/app.test/123')).toBe(false);
		expect(isValidNamedPackage('@org/app/v')).toBe(false);
		expect(isValidNamedPackage('@org/app/1/')).toBe(false);
		expect(isValidNamedPackage('@org/app/1.')).toBe(false);
		expect(isValidNamedPackage('@org/app/1.2')).toBe(false);
		expect(isValidNamedPackage('@org/app/1.2.3')).toBe(false);
		expect(isValidNamedPackage('@org1')).toBe(false);
		expect(isValidNamedPackage('-org/1')).toBe(false);
		expect(isValidNamedPackage('.org/1')).toBe(false);
		expect(isValidNamedPackage('org/1')).toBe(false);
		expect(isValidNamedPackage('@1org/-test/1')).toBe(false);
		expect(isValidNamedPackage('@-org/test/1')).toBe(false);
		expect(isValidNamedPackage('@o--rgtest/1')).toBe(false);
		expect(isValidNamedPackage('@o-rg/test/1-')).toBe(false);
		expect(isValidNamedPackage('@o-rg/test/1@')).toBe(false);
		expect(isValidNamedPackage('@org/tes--t/1')).toBe(false);
		expect(isValidNamedPackage('app@org')).toBe(false);
	});

	test('Valid/Invalid .move types', () => {
		expect(isValidNamedType('@org/app::string::String')).toBe(true);
		expect(isValidNamedType('@org/app/1::string::String')).toBe(true);
		expect(isValidNamedType('@org/app/123::string::String')).toBe(true);
		expect(isValidNamedType('@org/app-test/1::string::String')).toBe(true);
		expect(isValidNamedType('@org/1app/1::string::String')).toBe(true);
		expect(isValidNamedType('@org/1-app/1::string::String')).toBe(true);

		// failed scenarios.
		expect(isValidNamedType('@org/app.test/123::string::String')).toBe(false);
		expect(isValidNamedType('@org/app/v::string::String')).toBe(false);
		expect(isValidNamedType('@org/app/::string::String')).toBe(false);
		expect(isValidNamedType('@org/app/1/::string::String')).toBe(false);
		expect(isValidNamedType('@org/app/1.::string::String')).toBe(false);
		expect(isValidNamedType('@org/app/1.2::string::String')).toBe(false);
		expect(isValidNamedType('@org/app--::string::String')).toBe(false);
		expect(isValidNamedType('@org/ap--p::string::String')).toBe(false);
	});
});
