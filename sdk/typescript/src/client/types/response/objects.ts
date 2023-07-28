// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { CheckpointedObjectId } from './chain.js';
import type { SuiMovePackage } from './move.js';

export type OwnedObjectRef = {
	owner: ObjectOwner;
	reference: SuiObjectRef;
};

export type SuiObjectRef = {
	/** Base64 string representing the object digest */
	digest: string;
	/** Hex code as string representing the object id */
	objectId: string;
	/** Object version */
	version: number | string;
};

export type ObjectOwner =
	| {
			AddressOwner: string;
	  }
	| {
			ObjectOwner: string;
	  }
	| {
			Shared: {
				initial_shared_version: number;
			};
	  }
	| 'Immutable';

export type SuiObjectResponse = {
	data?: SuiObjectData;
	error?: SuiObjectResponseError;
};

export type SuiObjectResponseError = {
	code: string;
	error?: string;
	object_id?: string;
	parent_object_id?: string;
	version?: number;
	digest?: string;
};

export type SuiObjectData = {
	objectId: string;
	version: string;
	digest: string;
	/**
	 * Type of the object, default to be undefined unless SuiObjectDataOptions.showType is set to true
	 */
	type?: string;
	/**
	 * Move object content or package content, default to be undefined unless SuiObjectDataOptions.showContent is set to true
	 */
	content?: SuiParsedData;
	/**
	 * Move object content or package content in BCS bytes, default to be undefined unless SuiObjectDataOptions.showBcs is set to true
	 */
	bcs?: SuiRawData;
	/**
	 * The owner of this object. Default to be undefined unless SuiObjectDataOptions.showOwner is set to true
	 */
	owner?: ObjectOwner;
	/**
	 * The digest of the transaction that created or last mutated this object.
	 * Default to be undefined unless SuiObjectDataOptions.showPreviousTransaction is set to true
	 */
	previousTransaction?: string;
	/**
	 * The amount of SUI we would rebate if this object gets deleted.
	 * This number is re-calculated each time the object is mutated based on
	 * the present storage gas price.
	 * Default to be undefined unless SuiObjectDataOptions.showStorageRebate is set to true
	 */
	storageRebate?: string;
	/**
	 * Display metadata for this object, default to be undefined unless SuiObjectDataOptions.showDisplay is set to true
	 * This can also be None if the struct type does not have Display defined
	 * See more details in https://forums.sui.io/t/nft-object-display-proposal/4872
	 */
	display?: DisplayFieldsResponse;
};

export type SuiParsedData =
	| (SuiMoveObject & { dataType: 'moveObject' })
	| (SuiMovePackage & { dataType: 'package' });

export type SuiMoveObject = {
	/** Move type (e.g., "0x2::coin::Coin<0x2::sui::SUI>") */
	type: string;
	/** Fields and values stored inside the Move object */
	fields: ObjectContentFields;
	hasPublicTransfer: boolean;
};

export type SuiRawData =
	| (SuiRawMoveObject & { dataType: 'moveObject' })
	| (SuiRawMovePackage & { dataType: 'package' });

export type SuiRawMoveObject = {
	/** Move type (e.g., "0x2::coin::Coin<0x2::sui::SUI>") */
	type: string;
	hasPublicTransfer: boolean;
	version: number;
	bcsBytes: string;
};

export type SuiRawMovePackage = {
	id: string;
	/** A mapping from module name to Move bytecode enocded in base64*/
	moduleMap: Record<string, string>;
};

export type DisplayFieldsResponse = {
	data: Record<string, string> | null;
	error: SuiObjectResponseError | null;
};

export type ObjectContentFields = Record<string, any>;

export type PaginatedObjectsResponse = {
	data: SuiObjectResponse[];
	// TODO: remove union after 0.30.0 is released
	nextCursor: string | CheckpointedObjectId | null;
	hasNextPage: boolean;
};

export type ObjectRead =
	| {
			details: SuiObjectData;
			status: 'VersionFound';
	  }
	| {
			details: string;
			status: 'ObjectNotExists';
	  }
	| {
			details: SuiObjectRef;
			status: 'ObjectDeleted';
	  }
	| {
			details: string | number;
			status: 'VersionNotFound';
	  }
	| {
			details: {
				asked_version: number;
				latest_version: number;
				object_id: string;
			};
			status: 'VersionTooHigh';
	  };

export type DynamicFieldName = {
	type: string;
	value?: any;
};

export type DynamicFieldInfo = {
	name: DynamicFieldName;
	bcsName: string;
	type: DynamicFieldType;
	objectType: string;
	objectId: string;
	version: number;
	digest: string;
};

export type DynamicFieldPage = {
	data: DynamicFieldInfo[];
	nextCursor: string | null;
	hasNextPage: boolean;
};

export type DynamicFieldType = 'DynamicField' | 'DynamicObject';
