// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Output } from 'valibot';
import { literal, object, optional, string, url, uuid, variant } from 'valibot';

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
export type StashedRequestData = Output<typeof StashedRequestData>;

export const StashedRequest = object({
	id: string('`id` is required', [uuid()]),
	origin: string([url('`origin` must be a valid URL')]),
	name: optional(string()),
	payload: StashedRequestData,
});

export type StashedRequest = Output<typeof StashedRequest>;

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
export type StashedResponseData = Output<typeof StashedResponseData>;

export const StashedResponsePayload = variant('type', [
	object({
		type: literal('reject'),
	}),
	object({
		type: literal('resolve'),
		data: StashedResponseData,
	}),
]);
export type StashedResponsePayload = Output<typeof StashedResponsePayload>;

export const StashedResponse = object({
	id: string([uuid()]),
	source: literal('zksend-channel'),
	payload: StashedResponsePayload,
});
export type StashedResponse = Output<typeof StashedResponse>;

export type StashedRequestTypes = Record<string, any> & {
	[P in StashedRequestData as P['type']]: P;
};

export type StashedResponseTypes = {
	[P in StashedResponseData as P['type']]: P;
};
