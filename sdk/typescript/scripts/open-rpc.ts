// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export interface OpenRpcSpec {
	openrpc: string;
	info: {
		title: string;
		version: string;
		description: string;
		license: {
			name: string;
			url: string;
		};
		contact: {
			name: string;
			url: string;
			email: string;
		};
	};
	methods: OpenRpcMethod[];
	components: {
		schemas: Record<string, OpenRpcType>;
	};
}

export interface OpenRpcMethod {
	name: string;
	description?: string;
	tags?: {
		name: string;
	}[];
	params: OpenRpcParam[];
	result: OpenRpcParam;
	examples?: unknown[];
}

export interface OpenRpcParam {
	name: string;
	description?: string;
	required?: boolean;
	schema: OpenRpcType;
}

export type OpenRpcTypeRef = true | OpenRpcType[] | OpenRpcType;
export type OpenRpcType =
	| {
			description?: string;
			type: ('string' | 'integer' | 'number' | 'boolean' | 'array' | 'object' | 'null')[];
			additionalProperties?: boolean | OpenRpcTypeRef;
	  }
	| {
			$ref: string;
			description?: string;
			default?: unknown;
	  }
	| {
			type: 'null';
			description?: string;
			default?: null;
	  }
	| {
			type: 'array';
			items: OpenRpcTypeRef;
			description?: string;
			default?: unknown[];
			minItems?: number;
			maxItems?: number;
	  }
	| {
			type: 'string';
			default?: string;
			description?: string;
			enum?: string[];
	  }
	| {
			type: 'integer';
			description?: string;
			format: 'uint' | 'uint8' | 'uint16' | 'uint32' | 'uint64';
			minimum?: number;
			maximum?: number;
			default?: number;
	  }
	| {
			type: 'number';
			format?: 'double';
			description?: string;
			default?: number;
			minimum?: number;
			maximum?: number;
	  }
	| {
			type: 'boolean';
			description?: string;
			default?: boolean;
	  }
	| {
			oneOf: OpenRpcTypeRef[];
			description?: string;
			default?: unknown;
			properties?: Record<string, OpenRpcTypeRef>;
			required?: string[];
			additionalProperties?: boolean | OpenRpcTypeRef;
	  }
	| {
			anyOf: OpenRpcTypeRef[];
			description?: string;
			default?: unknown;
			properties?: Record<string, OpenRpcTypeRef>;
			required?: string[];
			additionalProperties?: boolean | OpenRpcTypeRef;
	  }
	| {
			allOf: OpenRpcTypeRef[];
			description?: string;
			default?: unknown;
	  }
	| {
			type: 'object';
			description?: string;
			properties?: Record<string, OpenRpcTypeRef>;
			required?: string[];
			additionalProperties?: boolean | OpenRpcTypeRef;
			default?: unknown;
	  }
	| {
			description?: string;
	  };
