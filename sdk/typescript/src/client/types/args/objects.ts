// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type SuiObjectResponseQuery = {
	filter?: SuiObjectDataFilter;
	options?: SuiObjectDataOptions;
};

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

/**
 * Config for fetching object data
 */
export type SuiObjectDataOptions = {
	/* Whether to fetch the object type, default to be true */
	showType?: boolean;
	/* Whether to fetch the object content, default to be false */
	showContent?: boolean;
	/* Whether to fetch the object content in BCS bytes, default to be false */
	showBcs?: boolean;
	/* Whether to fetch the object owner, default to be false */
	showOwner?: boolean;
	/* Whether to fetch the previous transaction digest, default to be false */
	showPreviousTransaction?: boolean;
	/* Whether to fetch the storage rebate, default to be false */
	showStorageRebate?: boolean;
	/* Whether to fetch the display metadata, default to be false */
	showDisplay?: boolean;
};
