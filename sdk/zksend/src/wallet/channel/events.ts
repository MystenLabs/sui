// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { InferOutput } from 'valibot';
import { literal, object, optional, pipe, string, url, uuid, variant } from 'valibot';

export const StashedRequestData = variant('type', [
	object({
		type: literal('connect'),
	}),
	object({
		type: literal('sign-transaction-block'),
		data: string('`data` is required'),
		address: string('`address` is required'),
	}),
	object({
		type: literal('sign-personal-message'),
		bytes: string('`bytes` is required'),
		address: string('`address` is required'),
	}),
]);
export type StashedRequestData = InferOutput<typeof StashedRequestData>;

export const StashedRequest = object({
	id: pipe(string('`id` is required'), uuid()),
	origin: pipe(string(), url('`origin` must be a valid URL')),
	name: optional(string()),
	payload: StashedRequestData,
});

export type StashedRequest = InferOutput<typeof StashedRequest>;

export const StashedResponseData = variant('type', [
	object({
		type: literal('connect'),
		address: string(),
	}),
	object({
		type: literal('sign-transaction-block'),
		bytes: string(),
		signature: string(),
	}),
	object({
		type: literal('sign-personal-message'),
		bytes: string(),
		signature: string(),
	}),
]);
export type StashedResponseData = InferOutput<typeof StashedResponseData>;

export const StashedResponsePayload = variant('type', [
	object({
		type: literal('reject'),
	}),
	object({
		type: literal('resolve'),
		data: StashedResponseData,
	}),
]);
export type StashedResponsePayload = InferOutput<typeof StashedResponsePayload>;

export const StashedResponse = object({
	id: pipe(string(), uuid()),
	source: literal('zksend-channel'),
	payload: StashedResponsePayload,
});
export type StashedResponse = InferOutput<typeof StashedResponse>;

export type StashedRequestTypes = Record<string, any> & {
	[P in StashedRequestData as P['type']]: P;
};

export type StashedResponseTypes = {
	[P in StashedResponseData as P['type']]: P;
};
