// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function objectToCamelCase(item: unknown): unknown {
	if (Array.isArray(item)) {
		return item.map((el: unknown) => objectToCamelCase(el));
	} else if (typeof item === 'function' || item !== Object(item)) {
		return item;
	}
	return Object.fromEntries(
		Object.entries(item as Record<string, unknown>).map(([key, value]: [string, unknown]) => [
			key.replace(/([-_][a-z])/gi, (c) => c.toUpperCase().replace(/[-_]/g, '')),
			objectToCamelCase(value),
		]),
	);
}
