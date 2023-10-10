// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import {
	any,
	array,
	assign,
	bigint,
	boolean,
	is,
	literal,
	nullable,
	number,
	object,
	optional,
	record,
	string,
	tuple,
	union,
	unknown,
} from 'superstruct';

import { ObjectOwner } from './common.js';

export const ObjectType = union([string(), literal('package')]);
export type ObjectType = Infer<typeof ObjectType>;

export const SuiObjectRef = object({
	/** Base64 string representing the object digest */
	digest: string(),
	/** Hex code as string representing the object id */
	objectId: string(),
	/** Object version */
	version: union([number(), string(), bigint()]),
});
export type SuiObjectRef = Infer<typeof SuiObjectRef>;

export const OwnedObjectRef = object({
	owner: ObjectOwner,
	reference: SuiObjectRef,
});
export type OwnedObjectRef = Infer<typeof OwnedObjectRef>;
export const TransactionEffectsModifiedAtVersions = object({
	objectId: string(),
	sequenceNumber: string(),
});

export const SuiGasData = object({
	payment: array(SuiObjectRef),
	/** Gas Object's owner */
	owner: string(),
	price: string(),
	budget: string(),
});
export type SuiGasData = Infer<typeof SuiGasData>;

export const SuiObjectInfo = assign(
	SuiObjectRef,
	object({
		type: string(),
		owner: ObjectOwner,
		previousTransaction: string(),
	}),
);
export type SuiObjectInfo = Infer<typeof SuiObjectInfo>;

export const ObjectContentFields = record(string(), any());
export type ObjectContentFields = Infer<typeof ObjectContentFields>;

export const MovePackageContent = record(string(), unknown());
export type MovePackageContent = Infer<typeof MovePackageContent>;

export const SuiMoveObject = object({
	/** Move type (e.g., "0x2::coin::Coin<0x2::sui::SUI>") */
	type: string(),
	/** Fields and values stored inside the Move object */
	fields: ObjectContentFields,
	hasPublicTransfer: boolean(),
});
export type SuiMoveObject = Infer<typeof SuiMoveObject>;

export const SuiMovePackage = object({
	/** A mapping from module name to disassembled Move bytecode */
	disassembled: MovePackageContent,
});
export type SuiMovePackage = Infer<typeof SuiMovePackage>;

export const SuiParsedData = union([
	assign(SuiMoveObject, object({ dataType: literal('moveObject') })),
	assign(SuiMovePackage, object({ dataType: literal('package') })),
]);
export type SuiParsedData = Infer<typeof SuiParsedData>;

export const SuiRawMoveObject = object({
	/** Move type (e.g., "0x2::coin::Coin<0x2::sui::SUI>") */
	type: string(),
	hasPublicTransfer: boolean(),
	version: string(),
	bcsBytes: string(),
});
export type SuiRawMoveObject = Infer<typeof SuiRawMoveObject>;

export const SuiRawMovePackage = object({
	id: string(),
	/** A mapping from module name to Move bytecode enocded in base64*/
	moduleMap: record(string(), string()),
});
export type SuiRawMovePackage = Infer<typeof SuiRawMovePackage>;

// TODO(chris): consolidate SuiRawParsedData and SuiRawObject using generics
export const SuiRawData = union([
	assign(SuiRawMoveObject, object({ dataType: literal('moveObject') })),
	assign(SuiRawMovePackage, object({ dataType: literal('package') })),
]);
export type SuiRawData = Infer<typeof SuiRawData>;

export const SUI_DECIMALS = 9;

export const MIST_PER_SUI = BigInt(1000000000);

export const SuiObjectResponseError = object({
	code: string(),
	error: optional(string()),
	object_id: optional(string()),
	parent_object_id: optional(string()),
	version: optional(string()),
	digest: optional(string()),
});
export type SuiObjectResponseError = Infer<typeof SuiObjectResponseError>;
export const DisplayFieldsResponse = object({
	data: nullable(optional(record(string(), string()))),
	error: nullable(optional(SuiObjectResponseError)),
});
export type DisplayFieldsResponse = Infer<typeof DisplayFieldsResponse>;
// TODO: remove after all envs support the new DisplayFieldsResponse;
export const DisplayFieldsBackwardCompatibleResponse = union([
	DisplayFieldsResponse,
	optional(record(string(), string())),
]);
export type DisplayFieldsBackwardCompatibleResponse = Infer<
	typeof DisplayFieldsBackwardCompatibleResponse
>;

export const SuiObjectData = object({
	objectId: string(),
	version: string(),
	digest: string(),
	/**
	 * Type of the object, default to be undefined unless SuiObjectDataOptions.showType is set to true
	 */
	type: nullable(optional(string())),
	/**
	 * Move object content or package content, default to be undefined unless SuiObjectDataOptions.showContent is set to true
	 */
	content: nullable(optional(SuiParsedData)),
	/**
	 * Move object content or package content in BCS bytes, default to be undefined unless SuiObjectDataOptions.showBcs is set to true
	 */
	bcs: nullable(optional(SuiRawData)),
	/**
	 * The owner of this object. Default to be undefined unless SuiObjectDataOptions.showOwner is set to true
	 */
	owner: nullable(optional(ObjectOwner)),
	/**
	 * The digest of the transaction that created or last mutated this object.
	 * Default to be undefined unless SuiObjectDataOptions.showPreviousTransaction is set to true
	 */
	previousTransaction: nullable(optional(string())),
	/**
	 * The amount of SUI we would rebate if this object gets deleted.
	 * This number is re-calculated each time the object is mutated based on
	 * the present storage gas price.
	 * Default to be undefined unless SuiObjectDataOptions.showStorageRebate is set to true
	 */
	storageRebate: nullable(optional(string())),
	/**
	 * Display metadata for this object, default to be undefined unless SuiObjectDataOptions.showDisplay is set to true
	 * This can also be None if the struct type does not have Display defined
	 * See more details in https://forums.sui.io/t/nft-object-display-proposal/4872
	 */
	display: nullable(optional(DisplayFieldsBackwardCompatibleResponse)),
});
export type SuiObjectData = Infer<typeof SuiObjectData>;

/**
 * Config for fetching object data
 */
export const SuiObjectDataOptions = object({
	/* Whether to fetch the object type, default to be true */
	showType: nullable(optional(boolean())),
	/* Whether to fetch the object content, default to be false */
	showContent: nullable(optional(boolean())),
	/* Whether to fetch the object content in BCS bytes, default to be false */
	showBcs: nullable(optional(boolean())),
	/* Whether to fetch the object owner, default to be false */
	showOwner: nullable(optional(boolean())),
	/* Whether to fetch the previous transaction digest, default to be false */
	showPreviousTransaction: nullable(optional(boolean())),
	/* Whether to fetch the storage rebate, default to be false */
	showStorageRebate: nullable(optional(boolean())),
	/* Whether to fetch the display metadata, default to be false */
	showDisplay: nullable(optional(boolean())),
});
export type SuiObjectDataOptions = Infer<typeof SuiObjectDataOptions>;

export const ObjectStatus = union([literal('Exists'), literal('notExists'), literal('Deleted')]);
export type ObjectStatus = Infer<typeof ObjectStatus>;

export const GetOwnedObjectsResponse = array(SuiObjectInfo);
export type GetOwnedObjectsResponse = Infer<typeof GetOwnedObjectsResponse>;

export const SuiObjectResponse = object({
	data: nullable(optional(SuiObjectData)),
	error: nullable(optional(SuiObjectResponseError)),
});
export type SuiObjectResponse = Infer<typeof SuiObjectResponse>;

export type Order = 'ascending' | 'descending';

/* -------------------------------------------------------------------------- */
/*                              Helper functions                              */
/* -------------------------------------------------------------------------- */

/* -------------------------- SuiObjectResponse ------------------------- */

export function getSuiObjectData(resp: SuiObjectResponse): SuiObjectData | null | undefined {
	return resp.data;
}

export function getObjectDeletedResponse(resp: SuiObjectResponse): SuiObjectRef | undefined {
	if (
		resp.error &&
		'object_id' in resp.error &&
		'version' in resp.error &&
		'digest' in resp.error
	) {
		const error = resp.error as SuiObjectResponseError;
		return {
			objectId: error.object_id,
			version: error.version,
			digest: error.digest,
		} as SuiObjectRef;
	}

	return undefined;
}

export function getObjectNotExistsResponse(resp: SuiObjectResponse): string | undefined {
	if (
		resp.error &&
		'object_id' in resp.error &&
		!('version' in resp.error) &&
		!('digest' in resp.error)
	) {
		return (resp.error as SuiObjectResponseError).object_id as string;
	}

	return undefined;
}

export function getObjectReference(
	resp: SuiObjectResponse | OwnedObjectRef,
): SuiObjectRef | undefined {
	if ('reference' in resp) {
		return resp.reference;
	}
	const exists = getSuiObjectData(resp);
	if (exists) {
		return {
			objectId: exists.objectId,
			version: exists.version,
			digest: exists.digest,
		};
	}
	return getObjectDeletedResponse(resp);
}

/* ------------------------------ SuiObjectRef ------------------------------ */

export function getObjectId(data: SuiObjectResponse | SuiObjectRef | OwnedObjectRef): string {
	if ('objectId' in data) {
		return data.objectId;
	}
	return (
		getObjectReference(data)?.objectId ?? getObjectNotExistsResponse(data as SuiObjectResponse)!
	);
}

export function getObjectVersion(
	data: SuiObjectResponse | SuiObjectRef | SuiObjectData,
): string | number | bigint | undefined {
	if ('version' in data) {
		return data.version;
	}
	return getObjectReference(data)?.version;
}

/* -------------------------------- SuiObject ------------------------------- */

export function isSuiObjectResponse(
	resp: SuiObjectResponse | SuiObjectData,
): resp is SuiObjectResponse {
	return (resp as SuiObjectResponse).data !== undefined;
}

/**
 * Deriving the object type from the object response
 * @returns 'package' if the object is a package, move object type(e.g., 0x2::coin::Coin<0x2::sui::SUI>)
 * if the object is a move object
 */
export function getObjectType(
	resp: SuiObjectResponse | SuiObjectData,
): ObjectType | null | undefined {
	const data = isSuiObjectResponse(resp) ? resp.data : resp;

	if (!data?.type && 'data' in resp) {
		if (data?.content?.dataType === 'package') {
			return 'package';
		}
		return getMoveObjectType(resp);
	}
	return data?.type;
}

export function getObjectPreviousTransactionDigest(
	resp: SuiObjectResponse,
): string | null | undefined {
	return getSuiObjectData(resp)?.previousTransaction;
}

export function getObjectOwner(
	resp: SuiObjectResponse | ObjectOwner,
): ObjectOwner | null | undefined {
	if (is(resp, ObjectOwner)) {
		return resp;
	}
	return getSuiObjectData(resp)?.owner;
}

export function getObjectDisplay(resp: SuiObjectResponse): DisplayFieldsResponse {
	const display = getSuiObjectData(resp)?.display;
	if (!display) {
		return { data: null, error: null };
	}
	if (is(display, DisplayFieldsResponse)) {
		return display;
	}
	return {
		data: display,
		error: null,
	};
}

export function getSharedObjectInitialVersion(
	resp: SuiObjectResponse | ObjectOwner,
): string | null | undefined {
	const owner = getObjectOwner(resp);
	if (owner && typeof owner === 'object' && 'Shared' in owner) {
		return owner.Shared.initial_shared_version;
	} else {
		return undefined;
	}
}

export function isSharedObject(resp: SuiObjectResponse | ObjectOwner): boolean {
	const owner = getObjectOwner(resp);
	return !!owner && typeof owner === 'object' && 'Shared' in owner;
}

export function isImmutableObject(resp: SuiObjectResponse | ObjectOwner): boolean {
	const owner = getObjectOwner(resp);
	return owner === 'Immutable';
}

export function getMoveObjectType(resp: SuiObjectResponse): string | undefined {
	return getMoveObject(resp)?.type;
}

export function getObjectFields(
	resp: SuiObjectResponse | SuiMoveObject | SuiObjectData,
): ObjectContentFields | undefined {
	if ('fields' in resp) {
		return resp.fields;
	}
	return getMoveObject(resp)?.fields;
}

export interface SuiObjectDataWithContent extends SuiObjectData {
	content: SuiParsedData;
}

function isSuiObjectDataWithContent(data: SuiObjectData): data is SuiObjectDataWithContent {
	return data.content !== undefined;
}

export function getMoveObject(data: SuiObjectResponse | SuiObjectData): SuiMoveObject | undefined {
	const suiObject = 'data' in data ? getSuiObjectData(data) : (data as SuiObjectData);

	if (
		!suiObject ||
		!isSuiObjectDataWithContent(suiObject) ||
		suiObject.content.dataType !== 'moveObject'
	) {
		return undefined;
	}

	return suiObject.content as SuiMoveObject;
}

export function hasPublicTransfer(data: SuiObjectResponse | SuiObjectData): boolean {
	return getMoveObject(data)?.hasPublicTransfer ?? false;
}

export function getMovePackageContent(
	data: SuiObjectResponse | SuiMovePackage,
): MovePackageContent | undefined {
	if ('disassembled' in data) {
		return data.disassembled;
	}
	const suiObject = getSuiObjectData(data);
	if (suiObject?.content?.dataType !== 'package') {
		return undefined;
	}
	return (suiObject.content as SuiMovePackage).disassembled;
}

export const CheckpointedObjectId = object({
	objectId: string(),
	atCheckpoint: optional(number()),
});
export type CheckpointedObjectId = Infer<typeof CheckpointedObjectId>;

export const PaginatedObjectsResponse = object({
	data: array(SuiObjectResponse),
	nextCursor: optional(nullable(string())),
	hasNextPage: boolean(),
});
export type PaginatedObjectsResponse = Infer<typeof PaginatedObjectsResponse>;

// mirrors sui_json_rpc_types:: SuiObjectDataFilter
export type SuiObjectDataFilter =
	| { MatchAll: SuiObjectDataFilter[] }
	| { MatchAny: SuiObjectDataFilter[] }
	| { MatchNone: SuiObjectDataFilter[] }
	| { Package: string }
	| { MoveModule: { package: string; module: string } }
	| { StructType: string }
	| { AddressOwner: string }
	| { ObjectOwner: string }
	| { ObjectId: string }
	| { ObjectIds: string[] }
	| { Version: string };

export type SuiObjectResponseQuery = {
	filter?: SuiObjectDataFilter;
	options?: SuiObjectDataOptions;
};

export const ObjectRead = union([
	object({
		details: SuiObjectData,
		status: literal('VersionFound'),
	}),
	object({
		details: string(),
		status: literal('ObjectNotExists'),
	}),
	object({
		details: SuiObjectRef,
		status: literal('ObjectDeleted'),
	}),
	object({
		details: tuple([string(), number()]),
		status: literal('VersionNotFound'),
	}),
	object({
		details: object({
			asked_version: number(),
			latest_version: number(),
			object_id: string(),
		}),
		status: literal('VersionTooHigh'),
	}),
]);
export type ObjectRead = Infer<typeof ObjectRead>;
