// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	FaucetResponse,
	SuiAddress,
	BatchFaucetResponse,
	BatchStatusFaucetResponse,
} from '../types/index.js';
import { FaucetRateLimitError } from '../utils/errors.js';
import { HttpHeaders } from './client.js';

export async function requestSuiFromFaucetV0(
	endpoint: string,
	recipient: SuiAddress,
	httpHeaders?: HttpHeaders,
): Promise<FaucetResponse> {
	const res = await fetch(endpoint + 'gas', {
		method: 'POST',
		body: JSON.stringify({
			FixedAmountRequest: {
				recipient,
			},
		}),
		headers: {
			'Content-Type': 'application/json',
			...(httpHeaders || {}),
		},
	});

	if (res.status === 429) {
		throw new FaucetRateLimitError(
			`Too many requests from this client have been sent to the faucet. Please retry later`,
		);
	}
	let parsed;
	try {
		parsed = await res.json();
	} catch (e) {
		throw new Error(
			`Encountered error when parsing response from faucet, error: ${e}, status ${res.status}, response ${res}`,
		);
	}
	if (parsed.error) {
		throw new Error(`Faucet returns error: ${parsed.error}`);
	}
	return parsed;
}

export async function requestSuiFromFaucetV1(
	endpoint: string,
	recipient: SuiAddress,
	httpHeaders?: HttpHeaders,
): Promise<BatchFaucetResponse> {
	const res = await fetch(endpoint + 'v1/gas', {
		method: 'POST',
		body: JSON.stringify({
			FixedAmountRequest: {
				recipient,
			},
		}),
		headers: {
			'Content-Type': 'application/json',
			...(httpHeaders || {}),
		},
	});

	if (res.status === 429) {
		throw new FaucetRateLimitError(
			`Too many requests from this client have been sent to the faucet. Please retry later`,
		);
	}
	let parsed;
	try {
		parsed = await res.json();
	} catch (e) {
		throw new Error(
			`Encountered error when parsing response from faucet, error: ${e}, status ${res.status}, response ${res}`,
		);
	}
	if (parsed.error) {
		throw new Error(`Faucet returns error: ${parsed.error}`);
	}
	return parsed;
}

export async function getFaucetRequestStatus(
	endpoint: string,
	task_id: string,
	httpHeaders?: HttpHeaders,
): Promise<BatchStatusFaucetResponse> {
	const res = await fetch(endpoint + 'v1/status', {
		method: 'POST',
		body: JSON.stringify({
			task_id: {
				task_id,
			},
		}),
		headers: {
			'Content-Type': 'application/json',
			...(httpHeaders || {}),
		},
	});

	if (res.status === 429) {
		throw new FaucetRateLimitError(
			`Too many requests from this client have been sent to the faucet. Please retry later`,
		);
	}
	let parsed;
	try {
		parsed = await res.json();
	} catch (e) {
		throw new Error(
			`Encountered error when parsing response from faucet, error: ${e}, status ${res.status}, response ${res}`,
		);
	}
	if (parsed.error) {
		throw new Error(`Faucet returns error: ${parsed.error}`);
	}
	return parsed;
}
