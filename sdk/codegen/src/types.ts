// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type TypeSignature =
	| 'U8'
	| 'U16'
	| 'U32'
	| 'U64'
	| 'U128'
	| 'U256'
	| 'Address'
	| 'Bool'
	| {
			Datatype: number;
	  }
	| {
			DatatypeInstantiation: [number, TypeSignature[]];
	  }
	| {
			Vector: TypeSignature;
	  }
	| {
			TypeParameter: number;
	  };

export interface DeserializedModule {
	version: number;
	self_module_handle_idx: number;
	module_handles: Array<{
		name: number;
		address: number;
	}>;
	datatype_handles: Array<{
		name: number;
		module: number;
		abilities: number;
		type_parameters: Array<{
			constraints: 0;
			is_phantom: false;
		}>;
	}>;
	function_handles: Array<{
		module: number;
		name: number;
		parameters: number;
		return_: number;
		type_parameters: Array<{
			constraints: 0;
			is_phantom: false;
		}>;
	}>;
	field_handles: Array<{
		owner: number;
		field: number;
	}>;
	friend_decls: Array<unknown>;
	struct_def_instantiations: Array<unknown>;
	function_instantiations: Array<unknown>;
	field_instantiations: Array<unknown>;
	signatures: Array<Array<TypeSignature>>;
	identifiers: Array<string>;
	address_identifiers: Array<string>;
	constant_pool: Array<{
		type_: TypeSignature;
		data: Array<number>;
	}>;
	metadata: Array<unknown>;
	struct_defs: Array<{
		struct_handle: number;
		field_information: {
			Declared?: Array<{
				name: number;
				signature: TypeSignature;
			}>;
		};
	}>;
	function_defs: Array<{
		function: number;
		visibility: 'Public' | 'Private' | 'Friend';
		is_entry: boolean;
		acquires_global_resources: Array<unknown>;
		flags: number;
		code: unknown;
	}>;
	enum_defs: Array<{
		enum_handle: number;
		variants: Array<{
			variant_name: number;
			fields: Array<{
				name: number;
				signature: TypeSignature;
			}>;
		}>;
	}>;
	enum_def_instantiations: Array<unknown>;
	variant_handles: Array<{
		enum_def: number;
		variant: number;
	}>;
	variant_instantiation_handles: Array<unknown>;
}
