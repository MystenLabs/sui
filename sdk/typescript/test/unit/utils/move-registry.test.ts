// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';

import { isValidNamedPackage, isValidNamedType } from '../../../src/utils';

describe('isValidNamedPackage', () => {
	test('Valid/Invalid .move names', () => {
		expect(isValidNamedPackage('app@org')).toBe(true);
		expect(isValidNamedPackage('app@org/v1')).toBe(true);
		expect(isValidNamedPackage('app@org/v123')).toBe(true);
		expect(isValidNamedPackage('app-test@org/v1')).toBe(true);
		expect(isValidNamedPackage('1app@org/v1')).toBe(true);
		expect(isValidNamedPackage('1-app@org/v1')).toBe(true);
		expect(isValidNamedPackage('test@o-rg/v1')).toBe(true);

		// failed scenarios.
		expect(isValidNamedPackage('app.test@org/v123')).toBe(false);
		expect(isValidNamedPackage('app@org/v')).toBe(false);
		expect(isValidNamedPackage('app@org/v1/')).toBe(false);
		expect(isValidNamedPackage('app@org/v1.')).toBe(false);
		expect(isValidNamedPackage('app@org/v1.2')).toBe(false);
		expect(isValidNamedPackage('app@org/v1.2.3')).toBe(false);
		expect(isValidNamedPackage('@org/v1')).toBe(false);
		expect(isValidNamedPackage('-org/v1')).toBe(false);
		expect(isValidNamedPackage('.org/v1')).toBe(false);
		expect(isValidNamedPackage('org/v1')).toBe(false);
		expect(isValidNamedPackage('-test@1org/v1')).toBe(false);
		expect(isValidNamedPackage('test@-org/v1')).toBe(false);
		expect(isValidNamedPackage('test@o--rg/v1')).toBe(false);
		expect(isValidNamedPackage('test@o-rg/v1-')).toBe(false);
		expect(isValidNamedPackage('test@o-rg/v1@')).toBe(false);
		expect(isValidNamedPackage('tes--t@org/v1')).toBe(false);
	});

	test('Valid/Invalid .move types', () => {
		expect(isValidNamedType('app@org::string::String')).toBe(true);
		expect(isValidNamedType('app@org/v1::string::String')).toBe(true);
		expect(isValidNamedType('app@org/v123::string::String')).toBe(true);
		expect(isValidNamedType('app-test@org/v1::string::String')).toBe(true);
		expect(isValidNamedType('1app@org/v1::string::String')).toBe(true);
		expect(isValidNamedType('1-app@org/v1::string::String')).toBe(true);

		// failed scenarios.
		expect(isValidNamedType('app.test@org/v123::string::String')).toBe(false);
		expect(isValidNamedType('app@org/v::string::String')).toBe(false);
		expect(isValidNamedType('app@org/v1/::string::String')).toBe(false);
		expect(isValidNamedType('app@org/v1.::string::String')).toBe(false);
		expect(isValidNamedType('app@org/v1.2::string::String')).toBe(false);
		expect(isValidNamedType('--app@org::string::String')).toBe(false);
		expect(isValidNamedType('ap--p@org::string::String')).toBe(false);
	});
});
