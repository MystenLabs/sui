// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const SUI_NS_NAME_REGEX =
	/^(?:[a-z0-9][a-z0-9-]{0,62}(?:\.[a-z0-9][a-z0-9-]{0,62})*)?@[a-z0-9][a-z0-9-]{0,62}$/i;
const SUI_NS_DOMAIN_REGEX = /^(?:[a-z0-9][a-z0-9-]{0,62}\.)+sui$/i;
const MAX_SUI_NS_NAME_LENGTH = 235;

export function isValidSuiNSName(name: string): boolean {
	if (name.length > MAX_SUI_NS_NAME_LENGTH) {
		return false;
	}

	if (name.includes('@')) {
		return SUI_NS_NAME_REGEX.test(name);
	}

	return SUI_NS_DOMAIN_REGEX.test(name);
}

export function normalizeSuiNSName(name: string, format: 'at' | 'dot' = 'at'): string {
	const lowerCase = name.toLowerCase();
	let parts;

	if (lowerCase.includes('@')) {
		if (!SUI_NS_NAME_REGEX.test(lowerCase)) {
			throw new Error(`Invalid SuiNS name ${name}`);
		}
		const [labels, domain] = lowerCase.split('@');
		parts = [...(labels ? labels.split('.') : []), domain];
	} else {
		if (!SUI_NS_DOMAIN_REGEX.test(lowerCase)) {
			throw new Error(`Invalid SuiNS name ${name}`);
		}
		parts = lowerCase.split('.').slice(0, -1);
	}

	if (format === 'dot') {
		return `${parts.join('.')}.sui`;
	}

	return `${parts.slice(0, -1).join('.')}@${parts[parts.length - 1]}`;
}
