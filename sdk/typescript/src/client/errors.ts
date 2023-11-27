// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const CODE_TO_ERROR_TYPE: Record<number, string> = {
	'-32700': 'ParseError',
	'-32600': 'InvalidRequest',
	'-32601': 'MethodNotFound',
	'-32602': 'InvalidParams',
	'-32603': 'InternalError',
};

export class SuiHTTPTransportError extends Error {}

export class JsonRpcError extends SuiHTTPTransportError {
	code: number;
	type: string;

	constructor(message: string, code: number) {
		super(message);
		this.code = code;
		this.type = CODE_TO_ERROR_TYPE[code] ?? 'ServerError';
	}
}

export class SuiHTTPStatusError extends SuiHTTPTransportError {
	status: number;
	statusText: string;

	constructor(message: string, status: number, statusText: string) {
		super(message);
		this.status = status;
		this.statusText = statusText;
	}
}
