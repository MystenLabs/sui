// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/* eslint-disable */

import gql from 'graphql-tag';

export type Maybe<T> = T | null;
export type InputMaybe<T> = Maybe<T>;
export type Exact<T extends { [key: string]: unknown }> = { [K in keyof T]: T[K] };
export type MakeOptional<T, K extends keyof T> = Omit<T, K> & { [SubKey in K]?: Maybe<T[SubKey]> };
export type MakeMaybe<T, K extends keyof T> = Omit<T, K> & { [SubKey in K]: Maybe<T[SubKey]> };
export type MakeEmpty<T extends { [key: string]: unknown }, K extends keyof T> = {
	[_ in K]?: never;
};
export type Incremental<T> =
	| T
	| { [P in keyof T]?: P extends ' $fragmentName' | '__typename' ? T[P] : never };
/** All built-in and custom scalars, mapped to their actual values */
export type Scalars = {
	ID: { input: string; output: string };
	String: { input: string; output: string };
	Boolean: { input: boolean; output: boolean };
	Int: { input: number; output: number };
	Float: { input: number; output: number };
	Base64: { input: any; output: any };
	BigInt: { input: any; output: any };
	DateTime: { input: any; output: any };
	/** Arbitrary JSON data. */
	JSON: { input: any; output: any };
	/**
	 * The contents of a Move Value, corresponding to the following recursive type:
	 *
	 * type MoveData =
	 *     { Address: SuiAddress }
	 *   | { UID:     SuiAddress }
	 *   | { Bool:    bool }
	 *   | { Number:  BigInt }
	 *   | { String:  string }
	 *   | { Vector:  [MoveData] }
	 *   | { Option:   MoveData? }
	 *   | { Struct:  [{ name: string, value: MoveData }] }
	 */
	MoveData: { input: any; output: any };
	/**
	 * The shape of a concrete Move Type (a type with all its type parameters instantiated with concrete types), corresponding to the following recursive type:
	 *
	 * type MoveTypeLayout =
	 *     "address"
	 *   | "bool"
	 *   | "u8" | "u16" | ... | "u256"
	 *   | { vector: MoveTypeLayout }
	 *   | { struct: [{ name: string, layout: MoveTypeLayout }] }
	 */
	MoveTypeLayout: { input: any; output: any };
	/**
	 * The signature of a concrete Move Type (a type with all its type parameters instantiated with concrete types, that contains no references), corresponding to the following recursive type:
	 *
	 * type MoveTypeSignature =
	 *     "address"
	 *   | "bool"
	 *   | "u8" | "u16" | ... | "u256"
	 *   | { vector: MoveTypeSignature }
	 *   | {
	 *       struct: {
	 *         package: string,
	 *         module: string,
	 *         type: string,
	 *         typeParameters: [MoveTypeSignature],
	 *       }
	 *     }
	 */
	MoveTypeSignature: { input: any; output: any };
	SuiAddress: { input: any; output: any };
};

export type Address = ObjectOwner & {
	__typename?: 'Address';
	balance?: Maybe<Balance>;
	balanceConnection?: Maybe<BalanceConnection>;
	coinConnection?: Maybe<CoinConnection>;
	defaultNameServiceName?: Maybe<Scalars['String']['output']>;
	location: Scalars['SuiAddress']['output'];
	objectConnection?: Maybe<ObjectConnection>;
	/** The `0x3::staking_pool::StakedSui` objects owned by the given address. */
	stakeConnection?: Maybe<StakeConnection>;
	/**
	 * Similar behavior to the `transactionBlockConnection` in Query but
	 * supports additional `AddressTransactionBlockRelationship` filter
	 */
	transactionBlockConnection?: Maybe<TransactionBlockConnection>;
};

export type AddressBalanceArgs = {
	type?: InputMaybe<Scalars['String']['input']>;
};

export type AddressBalanceConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type AddressCoinConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
	type?: InputMaybe<Scalars['String']['input']>;
};

export type AddressObjectConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<ObjectFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type AddressStakeConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type AddressTransactionBlockConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<TransactionBlockFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
	relation?: InputMaybe<AddressTransactionBlockRelationship>;
};

export enum AddressTransactionBlockRelationship {
	Paid = 'PAID',
	Recv = 'RECV',
	Sent = 'SENT',
	Sign = 'SIGN',
}

export type AuthenticatorStateUpdate = {
	__typename?: 'AuthenticatorStateUpdate';
	value: Scalars['String']['output'];
};

export type Balance = {
	__typename?: 'Balance';
	/** How many coins of this type constitute the balance */
	coinObjectCount?: Maybe<Scalars['Int']['output']>;
	/** Coin type for the balance, such as 0x2::sui::SUI */
	coinType?: Maybe<MoveType>;
	/** Total balance across all coin objects of the coin type */
	totalBalance?: Maybe<Scalars['BigInt']['output']>;
};

export type BalanceChange = {
	__typename?: 'BalanceChange';
	amount?: Maybe<Scalars['BigInt']['output']>;
	owner?: Maybe<Owner>;
};

export type BalanceConnection = {
	__typename?: 'BalanceConnection';
	/** A list of edges. */
	edges: Array<BalanceEdge>;
	/** A list of nodes. */
	nodes: Array<Balance>;
	/** Information to aid in pagination. */
	pageInfo: PageInfo;
};

/** An edge in a connection. */
export type BalanceEdge = {
	__typename?: 'BalanceEdge';
	/** A cursor for use in pagination */
	cursor: Scalars['String']['output'];
	/** The item at the end of the edge */
	node: Balance;
};

export type ChangeEpochTransaction = {
	__typename?: 'ChangeEpochTransaction';
	computationCharge?: Maybe<Scalars['BigInt']['output']>;
	epoch?: Maybe<Epoch>;
	storageCharge?: Maybe<Scalars['BigInt']['output']>;
	storageRebate?: Maybe<Scalars['BigInt']['output']>;
	timestamp?: Maybe<Scalars['DateTime']['output']>;
};

export type Checkpoint = {
	__typename?: 'Checkpoint';
	/**
	 * A 32-byte hash that uniquely identifies the checkpoint contents, encoded in Base58.
	 * This hash can be used to verify checkpoint contents by checking signatures against the committee,
	 * Hashing contents to match digest, and checking that the previous checkpoint digest matches.
	 */
	digest: Scalars['String']['output'];
	/**
	 * End of epoch data is only available on the final checkpoint of an epoch.
	 * This field provides information on the new committee and protocol version for the next epoch.
	 */
	endOfEpoch?: Maybe<EndOfEpochData>;
	epoch?: Maybe<Epoch>;
	/**
	 * This is a commitment by the committee at the end of epoch
	 * on the contents of the live object set at that time.
	 * This can be used to verify state snapshots.
	 */
	liveObjectSetDigest?: Maybe<Scalars['String']['output']>;
	/** Tracks the total number of transaction blocks in the network at the time of the checkpoint. */
	networkTotalTransactions?: Maybe<Scalars['Int']['output']>;
	/** The digest of the checkpoint at the previous sequence number. */
	previousCheckpointDigest?: Maybe<Scalars['String']['output']>;
	/**
	 * The computation and storage cost, storage rebate, and nonrefundable storage fee accumulated
	 * during this epoch, up to and including this checkpoint.
	 * These values increase monotonically across checkpoints in the same epoch.
	 */
	rollingGasSummary?: Maybe<GasCostSummary>;
	/** This checkpoint's position in the total order of finalised checkpoints, agreed upon by consensus. */
	sequenceNumber: Scalars['Int']['output'];
	/**
	 * The timestamp at which the checkpoint is agreed to have happened according to consensus.
	 * Transactions that access time in this checkpoint will observe this timestamp.
	 */
	timestamp?: Maybe<Scalars['DateTime']['output']>;
	transactionBlockConnection?: Maybe<TransactionBlockConnection>;
	/** This is an aggregation of signatures from a quorum of validators for the checkpoint proposal. */
	validatorSignature?: Maybe<Scalars['Base64']['output']>;
};

export type CheckpointTransactionBlockConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<TransactionBlockFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type CheckpointConnection = {
	__typename?: 'CheckpointConnection';
	/** A list of edges. */
	edges: Array<CheckpointEdge>;
	/** A list of nodes. */
	nodes: Array<Checkpoint>;
	/** Information to aid in pagination. */
	pageInfo: PageInfo;
};

/** An edge in a connection. */
export type CheckpointEdge = {
	__typename?: 'CheckpointEdge';
	/** A cursor for use in pagination */
	cursor: Scalars['String']['output'];
	/** The item at the end of the edge */
	node: Checkpoint;
};

export type CheckpointId = {
	digest?: InputMaybe<Scalars['String']['input']>;
	sequenceNumber?: InputMaybe<Scalars['Int']['input']>;
};

export type Coin = {
	__typename?: 'Coin';
	/** Convert the coin object into a Move object */
	asMoveObject?: Maybe<MoveObject>;
	/** Balance of the coin object */
	balance?: Maybe<Scalars['BigInt']['output']>;
	id: Scalars['ID']['output'];
};

export type CoinConnection = {
	__typename?: 'CoinConnection';
	/** A list of edges. */
	edges: Array<CoinEdge>;
	/** A list of nodes. */
	nodes: Array<Coin>;
	/** Information to aid in pagination. */
	pageInfo: PageInfo;
};

/** An edge in a connection. */
export type CoinEdge = {
	__typename?: 'CoinEdge';
	/** A cursor for use in pagination */
	cursor: Scalars['String']['output'];
	/** The item at the end of the edge */
	node: Coin;
};

export type CommitteeMember = {
	__typename?: 'CommitteeMember';
	authorityName?: Maybe<Scalars['String']['output']>;
	stakeUnit?: Maybe<Scalars['Int']['output']>;
};

export type ConsensusCommitPrologueTransaction = {
	__typename?: 'ConsensusCommitPrologueTransaction';
	epoch?: Maybe<Epoch>;
	round?: Maybe<Scalars['Int']['output']>;
	timestamp?: Maybe<Scalars['DateTime']['output']>;
};

export type EndOfEpochData = {
	__typename?: 'EndOfEpochData';
	newCommittee?: Maybe<Array<CommitteeMember>>;
	nextProtocolVersion?: Maybe<Scalars['Int']['output']>;
};

export type EndOfEpochTransaction = {
	__typename?: 'EndOfEpochTransaction';
	value: Scalars['String']['output'];
};

export type Epoch = {
	__typename?: 'Epoch';
	/** The epoch's corresponding checkpoints */
	checkpointConnection?: Maybe<CheckpointConnection>;
	/** The epoch's ending timestamp */
	endTimestamp?: Maybe<Scalars['DateTime']['output']>;
	/** The epoch's id as a sequence number that starts at 0 and it is incremented by one at every epoch change */
	epochId: Scalars['Int']['output'];
	/** The epoch's corresponding protocol configuration, including the feature flags and the configuration options */
	protocolConfigs?: Maybe<ProtocolConfigs>;
	/** The minimum gas price that a quorum of validators are guaranteed to sign a transaction for */
	referenceGasPrice?: Maybe<Scalars['BigInt']['output']>;
	/** The epoch's starting timestamp */
	startTimestamp?: Maybe<Scalars['DateTime']['output']>;
	/** The epoch's corresponding transaction blocks */
	transactionBlockConnection?: Maybe<TransactionBlockConnection>;
	/** Validator related properties, including the active validators */
	validatorSet?: Maybe<ValidatorSet>;
};

export type EpochCheckpointConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type EpochTransactionBlockConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<TransactionBlockFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type Event = {
	__typename?: 'Event';
	/** Base58 encoded bcs bytes of the Move event */
	bcs?: Maybe<Scalars['Base64']['output']>;
	/** Package, module, and type of the event */
	eventType?: Maybe<MoveType>;
	id: Scalars['ID']['output'];
	/** JSON string representation of the event */
	json?: Maybe<Scalars['String']['output']>;
	senders?: Maybe<Array<Address>>;
	/** Package id and module name of Move module that the event was emitted in */
	sendingModuleId?: Maybe<MoveModuleId>;
	/** UTC timestamp in milliseconds since epoch (1/1/1970) */
	timestamp?: Maybe<Scalars['DateTime']['output']>;
};

export type EventConnection = {
	__typename?: 'EventConnection';
	/** A list of edges. */
	edges: Array<EventEdge>;
	/** A list of nodes. */
	nodes: Array<Event>;
	/** Information to aid in pagination. */
	pageInfo: PageInfo;
};

/** An edge in a connection. */
export type EventEdge = {
	__typename?: 'EventEdge';
	/** A cursor for use in pagination */
	cursor: Scalars['String']['output'];
	/** The item at the end of the edge */
	node: Event;
};

export type EventFilter = {
	emittingModule?: InputMaybe<Scalars['String']['input']>;
	emittingPackage?: InputMaybe<Scalars['SuiAddress']['input']>;
	eventModule?: InputMaybe<Scalars['String']['input']>;
	eventPackage?: InputMaybe<Scalars['SuiAddress']['input']>;
	eventType?: InputMaybe<Scalars['String']['input']>;
	sender?: InputMaybe<Scalars['SuiAddress']['input']>;
	transactionDigest?: InputMaybe<Scalars['String']['input']>;
};

export enum ExecutionStatus {
	Failure = 'FAILURE',
	Success = 'SUCCESS',
}

/**
 * Groups of features served by the RPC service.  The GraphQL Service can be configured to enable
 * or disable these features.
 */
export enum Feature {
	/** Statistics about how the network was running (TPS, top packages, APY, etc) */
	Analytics = 'ANALYTICS',
	/** Coin metadata, per-address coin and balance information. */
	Coins = 'COINS',
	/** Querying an object's dynamic fields. */
	DynamicFields = 'DYNAMIC_FIELDS',
	/** SuiNS name and reverse name look-up. */
	NameService = 'NAME_SERVICE',
	/** Transaction and Event subscriptions. */
	Subscriptions = 'SUBSCRIPTIONS',
	/**
	 * Information about the system that changes from epoch to epoch (protocol config, committee,
	 * reference gas price).
	 */
	SystemState = 'SYSTEM_STATE',
}

export type GasCostSummary = {
	__typename?: 'GasCostSummary';
	computationCost?: Maybe<Scalars['BigInt']['output']>;
	nonRefundableStorageFee?: Maybe<Scalars['BigInt']['output']>;
	storageCost?: Maybe<Scalars['BigInt']['output']>;
	storageRebate?: Maybe<Scalars['BigInt']['output']>;
};

export type GasEffects = {
	__typename?: 'GasEffects';
	gasObject?: Maybe<Object>;
	gasSummary?: Maybe<GasCostSummary>;
};

export type GasInput = {
	__typename?: 'GasInput';
	/** The maximum number of gas units that can be expended by executing this transaction */
	gasBudget?: Maybe<Scalars['BigInt']['output']>;
	/** Objects used to pay for a transaction's execution and storage */
	gasPayment?: Maybe<ObjectConnection>;
	/** An unsigned integer specifying the number of native tokens per gas unit this transaction will pay */
	gasPrice?: Maybe<Scalars['BigInt']['output']>;
	/** Address of the owner of the gas object(s) used */
	gasSponsor?: Maybe<Address>;
};

export type GasInputGasPaymentArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type GenesisTransaction = {
	__typename?: 'GenesisTransaction';
	objects?: Maybe<Array<Scalars['SuiAddress']['output']>>;
};

/** Information used by a package to link to a specific version of its dependency. */
export type Linkage = {
	__typename?: 'Linkage';
	/** The ID on-chain of the first version of the dependency. */
	originalId: Scalars['SuiAddress']['output'];
	/** The ID on-chain of the version of the dependency that this package depends on. */
	upgradedId: Scalars['SuiAddress']['output'];
	/** The version of the dependency that this package depends on. */
	version: Scalars['Int']['output'];
};

/**
 * Represents a module in Move, a library that defines struct types
 * and functions that operate on these types.
 */
export type MoveModule = {
	__typename?: 'MoveModule';
	fileFormatVersion: Scalars['Int']['output'];
};

export type MoveModuleConnection = {
	__typename?: 'MoveModuleConnection';
	/** A list of edges. */
	edges: Array<MoveModuleEdge>;
	/** A list of nodes. */
	nodes: Array<MoveModule>;
	/** Information to aid in pagination. */
	pageInfo: PageInfo;
};

/** An edge in a connection. */
export type MoveModuleEdge = {
	__typename?: 'MoveModuleEdge';
	/** A cursor for use in pagination */
	cursor: Scalars['String']['output'];
	/** The item at the end of the edge */
	node: MoveModule;
};

export type MoveModuleId = {
	__typename?: 'MoveModuleId';
	name: Scalars['String']['output'];
	/** The package that this Move module was defined in */
	package: MovePackage;
};

export type MoveObject = {
	__typename?: 'MoveObject';
	/** Attempts to convert the Move object into a Coin */
	asCoin?: Maybe<Coin>;
	/**
	 * Attempts to convert the Move object into an Object
	 * This provides additional information such as version and digest on the top-level
	 */
	asObject?: Maybe<Object>;
	/** Attempts to convert the Move object into a Stake */
	asStake?: Maybe<Stake>;
	/**
	 * Displays the contents of the MoveObject in a json string and through graphql types
	 * Also provides the flat representation of the type signature, and the bcs of the corresponding data
	 */
	contents?: Maybe<MoveValue>;
	/** Determines whether a tx can transfer this object */
	hasPublicTransfer?: Maybe<Scalars['Boolean']['output']>;
};

export type MovePackage = {
	__typename?: 'MovePackage';
	asObject?: Maybe<Object>;
	/**
	 * BCS representation of the package's modules.  Modules appear as a sequence of pairs (module
	 * name, followed by module bytes), in alphabetic order by module name.
	 */
	bcs?: Maybe<Scalars['Base64']['output']>;
	/** The transitive dependencies of this package. */
	linkage?: Maybe<Array<Linkage>>;
	/**
	 * A representation of the module called `name` in this package, including the
	 * structs and functions it defines.
	 */
	module?: Maybe<MoveModule>;
	/** Paginate through the MoveModules defined in this package. */
	moduleConnection?: Maybe<MoveModuleConnection>;
	/** The (previous) versions of this package that introduced its types. */
	typeOrigins?: Maybe<Array<TypeOrigin>>;
};

export type MovePackageModuleArgs = {
	name: Scalars['String']['input'];
};

export type MovePackageModuleConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

/** Represents concrete types (no type parameters, no references) */
export type MoveType = {
	__typename?: 'MoveType';
	/** Structured representation of the "shape" of values that match this type. */
	layout: Scalars['MoveTypeLayout']['output'];
	/** Flat representation of the type signature, as a displayable string. */
	repr: Scalars['String']['output'];
	/** Structured representation of the type signature. */
	signature: Scalars['MoveTypeSignature']['output'];
};

export type MoveValue = {
	__typename?: 'MoveValue';
	bcs: Scalars['Base64']['output'];
	/** Structured contents of a Move value. */
	data: Scalars['MoveData']['output'];
	/**
	 * Representation of a Move value in JSON, where:
	 *
	 * - Addresses and UIDs are represented in canonical form, as JSON strings.
	 * - Bools are represented by JSON boolean literals.
	 * - u8, u16, and u32 are represented as JSON numbers.
	 * - u64, u128, and u256 are represented as JSON strings.
	 * - Vectors are represented by JSON arrays.
	 * - Structs are represented by JSON objects.
	 * - Empty optional values are represented by `null`.
	 *
	 * This form is offered as a less verbose convenience in cases where the layout of the type is
	 * known by the client.
	 */
	json: Scalars['JSON']['output'];
	type: MoveType;
};

export type Object = ObjectOwner & {
	__typename?: 'Object';
	/** Attempts to convert the object into a MoveObject */
	asMoveObject?: Maybe<MoveObject>;
	/** Attempts to convert the object into a MovePackage */
	asMovePackage?: Maybe<MovePackage>;
	/** The balance of coin objects of a particular coin type owned by the object. */
	balance?: Maybe<Balance>;
	/** The balances of all coin types owned by the object. Coins of the same type are grouped together into one Balance. */
	balanceConnection?: Maybe<BalanceConnection>;
	/** The Base64 encoded bcs serialization of the object's content. */
	bcs?: Maybe<Scalars['Base64']['output']>;
	/** The `0x2::sui::Coin` objects owned by the given object. */
	coinConnection?: Maybe<CoinConnection>;
	/** The domain that a user address has explicitly configured as their default domain */
	defaultNameServiceName?: Maybe<Scalars['String']['output']>;
	/** 32-byte hash that identifies the object's current contents, encoded as a Base58 string. */
	digest: Scalars['String']['output'];
	/**
	 * Objects can either be immutable, shared, owned by an address,
	 * or are child objects (part of a dynamic field)
	 */
	kind?: Maybe<ObjectKind>;
	/** The address of the object, named as such to avoid conflict with the address type. */
	location: Scalars['SuiAddress']['output'];
	/** The objects owned by this object */
	objectConnection?: Maybe<ObjectConnection>;
	/** The Address or Object that owns this Object.  Immutable and Shared Objects do not have owners. */
	owner?: Maybe<Owner>;
	/** The transaction block that created this version of the object. */
	previousTransactionBlock?: Maybe<TransactionBlock>;
	/** The `0x3::staking_pool::StakedSui` objects owned by the given object. */
	stakeConnection?: Maybe<StakeConnection>;
	/**
	 * The amount of SUI we would rebate if this object gets deleted or mutated.
	 * This number is recalculated based on the present storage gas price.
	 */
	storageRebate?: Maybe<Scalars['BigInt']['output']>;
	version: Scalars['Int']['output'];
};

export type ObjectBalanceArgs = {
	type?: InputMaybe<Scalars['String']['input']>;
};

export type ObjectBalanceConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type ObjectCoinConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
	type?: InputMaybe<Scalars['String']['input']>;
};

export type ObjectObjectConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<ObjectFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type ObjectStakeConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type ObjectChange = {
	__typename?: 'ObjectChange';
	idCreated?: Maybe<Scalars['Boolean']['output']>;
	idDeleted?: Maybe<Scalars['Boolean']['output']>;
	inputState?: Maybe<Object>;
	outputState?: Maybe<Object>;
};

export type ObjectConnection = {
	__typename?: 'ObjectConnection';
	/** A list of edges. */
	edges: Array<ObjectEdge>;
	/** A list of nodes. */
	nodes: Array<Object>;
	/** Information to aid in pagination. */
	pageInfo: PageInfo;
};

/** An edge in a connection. */
export type ObjectEdge = {
	__typename?: 'ObjectEdge';
	/** A cursor for use in pagination */
	cursor: Scalars['String']['output'];
	/** The item at the end of the edge */
	node: Object;
};

export type ObjectFilter = {
	module?: InputMaybe<Scalars['String']['input']>;
	objectIds?: InputMaybe<Array<Scalars['SuiAddress']['input']>>;
	objectKeys?: InputMaybe<Array<ObjectKey>>;
	owner?: InputMaybe<Scalars['SuiAddress']['input']>;
	package?: InputMaybe<Scalars['SuiAddress']['input']>;
	ty?: InputMaybe<Scalars['String']['input']>;
};

export type ObjectKey = {
	objectId: Scalars['SuiAddress']['input'];
	version: Scalars['Int']['input'];
};

export enum ObjectKind {
	Child = 'CHILD',
	Immutable = 'IMMUTABLE',
	Owned = 'OWNED',
	Shared = 'SHARED',
}

export type ObjectOwner = {
	balance?: Maybe<Balance>;
	balanceConnection?: Maybe<BalanceConnection>;
	coinConnection?: Maybe<CoinConnection>;
	defaultNameServiceName?: Maybe<Scalars['String']['output']>;
	location: Scalars['SuiAddress']['output'];
	objectConnection?: Maybe<ObjectConnection>;
	stakeConnection?: Maybe<StakeConnection>;
};

export type ObjectOwnerBalanceArgs = {
	type?: InputMaybe<Scalars['String']['input']>;
};

export type ObjectOwnerBalanceConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type ObjectOwnerCoinConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
	type?: InputMaybe<Scalars['String']['input']>;
};

export type ObjectOwnerObjectConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<ObjectFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type ObjectOwnerStakeConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type Owner = ObjectOwner & {
	__typename?: 'Owner';
	asAddress?: Maybe<Address>;
	asObject?: Maybe<Object>;
	balance?: Maybe<Balance>;
	balanceConnection?: Maybe<BalanceConnection>;
	coinConnection?: Maybe<CoinConnection>;
	defaultNameServiceName?: Maybe<Scalars['String']['output']>;
	location: Scalars['SuiAddress']['output'];
	objectConnection?: Maybe<ObjectConnection>;
	/** The stake objects for the given address */
	stakeConnection?: Maybe<StakeConnection>;
};

export type OwnerBalanceArgs = {
	type?: InputMaybe<Scalars['String']['input']>;
};

export type OwnerBalanceConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type OwnerCoinConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
	type?: InputMaybe<Scalars['String']['input']>;
};

export type OwnerObjectConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<ObjectFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type OwnerStakeConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

/** Information about pagination in a connection */
export type PageInfo = {
	__typename?: 'PageInfo';
	/** When paginating forwards, the cursor to continue. */
	endCursor?: Maybe<Scalars['String']['output']>;
	/** When paginating forwards, are there more items? */
	hasNextPage: Scalars['Boolean']['output'];
	/** When paginating backwards, are there more items? */
	hasPreviousPage: Scalars['Boolean']['output'];
	/** When paginating backwards, the cursor to continue. */
	startCursor?: Maybe<Scalars['String']['output']>;
};

export type ProgrammableTransaction = {
	__typename?: 'ProgrammableTransaction';
	value: Scalars['String']['output'];
};

/** A single protocol configuration value. */
export type ProtocolConfigAttr = {
	__typename?: 'ProtocolConfigAttr';
	key: Scalars['String']['output'];
	value: Scalars['String']['output'];
};

/** Whether or not a single feature is enabled in the protocol config. */
export type ProtocolConfigFeatureFlag = {
	__typename?: 'ProtocolConfigFeatureFlag';
	key: Scalars['String']['output'];
	value: Scalars['Boolean']['output'];
};

/**
 * Constants that control how the chain operates.
 *
 * These can only change during protocol upgrades which happen on epoch boundaries.
 */
export type ProtocolConfigs = {
	__typename?: 'ProtocolConfigs';
	/** Query for the value of the configuration with name `key`. */
	config?: Maybe<ProtocolConfigAttr>;
	/**
	 * List all available configurations and their values.  These configurations can take any value
	 * (but they will all be represented in string form), and do not include feature flags.
	 */
	configs: Array<ProtocolConfigAttr>;
	/** Query for the state of the feature flag with name `key`. */
	featureFlag?: Maybe<ProtocolConfigFeatureFlag>;
	/**
	 * List all available feature flags and their values.  Feature flags are a form of boolean
	 * configuration that are usually used to gate features while they are in development.  Once a
	 * flag has been enabled, it is rare for it to be disabled.
	 */
	featureFlags: Array<ProtocolConfigFeatureFlag>;
	/**
	 * The protocol is not required to change on every epoch boundary, so the protocol version
	 * tracks which change to the protocol these configs are from.
	 */
	protocolVersion: Scalars['Int']['output'];
};

/**
 * Constants that control how the chain operates.
 *
 * These can only change during protocol upgrades which happen on epoch boundaries.
 */
export type ProtocolConfigsConfigArgs = {
	key: Scalars['String']['input'];
};

/**
 * Constants that control how the chain operates.
 *
 * These can only change during protocol upgrades which happen on epoch boundaries.
 */
export type ProtocolConfigsFeatureFlagArgs = {
	key: Scalars['String']['input'];
};

export type Query = {
	__typename?: 'Query';
	address?: Maybe<Address>;
	/**
	 * First four bytes of the network's genesis checkpoint digest (uniquely identifies the
	 * network).
	 */
	chainIdentifier: Scalars['String']['output'];
	checkpoint?: Maybe<Checkpoint>;
	checkpointConnection?: Maybe<CheckpointConnection>;
	epoch?: Maybe<Epoch>;
	eventConnection?: Maybe<EventConnection>;
	latestSuiSystemState: SuiSystemStateSummary;
	object?: Maybe<Object>;
	objectConnection?: Maybe<ObjectConnection>;
	owner?: Maybe<ObjectOwner>;
	protocolConfig: ProtocolConfigs;
	/** Resolves the owner address of the provided domain name */
	resolveNameServiceAddress?: Maybe<Address>;
	/** Configuration for this RPC service */
	serviceConfig: ServiceConfig;
	transactionBlock?: Maybe<TransactionBlock>;
	transactionBlockConnection?: Maybe<TransactionBlockConnection>;
};

export type QueryAddressArgs = {
	address: Scalars['SuiAddress']['input'];
};

export type QueryCheckpointArgs = {
	id?: InputMaybe<CheckpointId>;
};

export type QueryCheckpointConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type QueryEpochArgs = {
	id?: InputMaybe<Scalars['Int']['input']>;
};

export type QueryEventConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter: EventFilter;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type QueryObjectArgs = {
	address: Scalars['SuiAddress']['input'];
	version?: InputMaybe<Scalars['Int']['input']>;
};

export type QueryObjectConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<ObjectFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

export type QueryOwnerArgs = {
	address: Scalars['SuiAddress']['input'];
};

export type QueryProtocolConfigArgs = {
	protocolVersion?: InputMaybe<Scalars['Int']['input']>;
};

export type QueryResolveNameServiceAddressArgs = {
	name: Scalars['String']['input'];
};

export type QueryTransactionBlockArgs = {
	digest: Scalars['String']['input'];
};

export type QueryTransactionBlockConnectionArgs = {
	after?: InputMaybe<Scalars['String']['input']>;
	before?: InputMaybe<Scalars['String']['input']>;
	filter?: InputMaybe<TransactionBlockFilter>;
	first?: InputMaybe<Scalars['Int']['input']>;
	last?: InputMaybe<Scalars['Int']['input']>;
};

/** Information about whether epoch changes are using safe mode. */
export type SafeMode = {
	__typename?: 'SafeMode';
	/**
	 * Whether safe mode was used for the last epoch change.  The system will retry a full epoch
	 * change on every epoch boundary and automatically reset this flag if so.
	 */
	enabled?: Maybe<Scalars['Boolean']['output']>;
	/**
	 * Accumulated fees for computation and cost that have not been added to the various reward
	 * pools, because the full epoch change did not happen.
	 */
	gasSummary?: Maybe<GasCostSummary>;
};

export type ServiceConfig = {
	__typename?: 'ServiceConfig';
	/** List of all features that are enabled on this GraphQL service. */
	enabledFeatures: Array<Feature>;
	/** Check whether `feature` is enabled on this GraphQL service. */
	isEnabled: Scalars['Boolean']['output'];
	/**
	 * Maximum estimated cost of a database query used to serve a GraphQL request.  This is
	 * measured in the same units that the database uses in EXPLAIN queries.
	 */
	maxDbQueryCost: Scalars['BigInt']['output'];
	/** The maximum depth a GraphQL query can be to be accepted by this service. */
	maxQueryDepth: Scalars['Int']['output'];
	/** Maximum number of fragments a query can define */
	maxQueryFragments: Scalars['Int']['output'];
	/** The maximum number of nodes (field names) the service will accept in a single query. */
	maxQueryNodes: Scalars['Int']['output'];
	/** Maximum number of variables a query can define */
	maxQueryVariables: Scalars['Int']['output'];
	/** Maximum time in milliseconds that will be spent to serve one request. */
	requestTimeoutMs: Scalars['BigInt']['output'];
};

export type ServiceConfigIsEnabledArgs = {
	feature: Feature;
};

export type Stake = {
	__typename?: 'Stake';
	/** The epoch at which this stake became active */
	activeEpoch?: Maybe<Epoch>;
	/** The corresponding StakedSui Move object */
	asMoveObject?: Maybe<MoveObject>;
	/**
	 * The estimated reward for this stake object, computed as the
	 * value of multiplying the principal value with the ratio between the initial stake rate and the current rate
	 */
	estimatedReward?: Maybe<Scalars['BigInt']['output']>;
	/** Stake object address */
	id: Scalars['ID']['output'];
	/** The amount of SUI that is used to stake */
	principal?: Maybe<Scalars['BigInt']['output']>;
	/** The epoch at which this object was requested to join a stake pool */
	requestEpoch?: Maybe<Epoch>;
	/** The status of this stake object: Active, Pending, Unstaked */
	status?: Maybe<StakeStatus>;
};

export type StakeConnection = {
	__typename?: 'StakeConnection';
	/** A list of edges. */
	edges: Array<StakeEdge>;
	/** A list of nodes. */
	nodes: Array<Stake>;
	/** Information to aid in pagination. */
	pageInfo: PageInfo;
};

/** An edge in a connection. */
export type StakeEdge = {
	__typename?: 'StakeEdge';
	/** A cursor for use in pagination */
	cursor: Scalars['String']['output'];
	/** The item at the end of the edge */
	node: Stake;
};

export enum StakeStatus {
	/** The stake object is active in a staking pool and it is generating rewards */
	Active = 'ACTIVE',
	/** The stake awaits to join a staking pool in the next epoch */
	Pending = 'PENDING',
	/** The stake is no longer active in any staking pool */
	Unstaked = 'UNSTAKED',
}

/** Parameters related to subsiding staking rewards */
export type StakeSubsidy = {
	__typename?: 'StakeSubsidy';
	/**
	 * SUI set aside for stake subsidies -- reduces over time as stake subsidies are paid out over
	 * time.
	 */
	balance?: Maybe<Scalars['BigInt']['output']>;
	/** Amount of stake subsidy deducted from the balance per distribution -- decays over time. */
	currentDistributionAmount?: Maybe<Scalars['BigInt']['output']>;
	/**
	 * Percentage of the current distribution amount to deduct at the end of the current subsidy
	 * period, expressed in basis points.
	 */
	decreaseRate?: Maybe<Scalars['Int']['output']>;
	/**
	 * Number of times stake subsidies have been distributed subsidies are distributed with other
	 * staking rewards, at the end of the epoch.
	 */
	distributionCounter?: Maybe<Scalars['Int']['output']>;
	/**
	 * Maximum number of stake subsidy distributions that occur with the same distribution amount
	 * (before the amount is reduced).
	 */
	periodLength?: Maybe<Scalars['Int']['output']>;
};

/** SUI set aside to account for objects stored on-chain. */
export type StorageFund = {
	__typename?: 'StorageFund';
	/**
	 * The portion of the storage fund that will never be refunded through storage rebates.
	 *
	 * The system maintains an invariant that the sum of all storage fees into the storage fund is
	 * equal to the sum of of all storage rebates out, the total storage rebates remaining, and the
	 * non-refundable balance.
	 */
	nonRefundableBalance?: Maybe<Scalars['BigInt']['output']>;
	/** Sum of storage rebates of live objects on chain. */
	totalObjectStorageRebates?: Maybe<Scalars['BigInt']['output']>;
};

/**
 * Aspects that affect the running of the system that are managed by the validators either
 * directly, or through system transactions.
 */
export type SuiSystemStateSummary = {
	__typename?: 'SuiSystemStateSummary';
	/** The epoch for which this is the system state. */
	epoch?: Maybe<Epoch>;
	/**
	 * Configuration for how the chain operates that can change from epoch to epoch (due to a
	 * protocol version upgrade).
	 */
	protocolConfigs?: Maybe<ProtocolConfigs>;
	/** The minimum gas price that a quorum of validators are guaranteed to sign a transaction for. */
	referenceGasPrice?: Maybe<Scalars['BigInt']['output']>;
	/**
	 * Information about whether last epoch change used safe mode, which happens if the full epoch
	 * change logic fails for some reason.
	 */
	safeMode?: Maybe<SafeMode>;
	/** Parameters related to subsiding staking rewards */
	stakeSubsidy?: Maybe<StakeSubsidy>;
	/** The start of the current epoch. */
	startTimestamp?: Maybe<Scalars['DateTime']['output']>;
	/** SUI set aside to account for objects stored on-chain, at the start of the epoch. */
	storageFund?: Maybe<StorageFund>;
	/** Details of the system that are decided during genesis. */
	systemParameters?: Maybe<SystemParameters>;
	/**
	 * The value of the `version` field of `0x5`, the `0x3::sui::SuiSystemState` object.  This
	 * version changes whenever the fields contained in the system state object (held in a dynamic
	 * field attached to `0x5`) change.
	 */
	systemStateVersion?: Maybe<Scalars['BigInt']['output']>;
	/** Details of the currently active validators and pending changes to that set. */
	validatorSet?: Maybe<ValidatorSet>;
};

/** Details of the system that are decided during genesis. */
export type SystemParameters = {
	__typename?: 'SystemParameters';
	/** Target duration of an epoch, in milliseconds. */
	durationMs?: Maybe<Scalars['BigInt']['output']>;
	/** The maximum number of active validators that the system supports. */
	maxValidatorCount?: Maybe<Scalars['Int']['output']>;
	/** The minimum number of active validators that the system supports. */
	minValidatorCount?: Maybe<Scalars['Int']['output']>;
	/** Minimum stake needed to become a new validator. */
	minValidatorJoiningStake?: Maybe<Scalars['BigInt']['output']>;
	/** The epoch at which stake subsidies start being paid out. */
	stakeSubsidyStartEpoch?: Maybe<Scalars['Int']['output']>;
	/**
	 * The number of epochs that a validator has to recover from having less than
	 * `validatorLowStakeThreshold` stake.
	 */
	validatorLowStakeGracePeriod?: Maybe<Scalars['BigInt']['output']>;
	/**
	 * Validators with stake below this threshold will enter the grace period (see
	 * `validatorLowStakeGracePeriod`), after which they are removed from the active validator set.
	 */
	validatorLowStakeThreshold?: Maybe<Scalars['BigInt']['output']>;
	/**
	 * Validators with stake below this threshold will be removed from the the active validator set
	 * at the next epoch boundary, without a grace period.
	 */
	validatorVeryLowStakeThreshold?: Maybe<Scalars['BigInt']['output']>;
};

export type TransactionBlock = {
	__typename?: 'TransactionBlock';
	/**
	 * The transaction block data in BCS format.
	 * This includes data on the sender, inputs, sponsor, gas inputs, individual transactions, and user signatures.
	 */
	bcs?: Maybe<Scalars['Base64']['output']>;
	/**
	 * A 32-byte hash that uniquely identifies the transaction block contents, encoded in Base58.
	 * This serves as a unique id for the block on chain
	 */
	digest: Scalars['String']['output'];
	/** The effects field captures the results to the chain of executing this transaction */
	effects?: Maybe<TransactionBlockEffects>;
	/**
	 * This field is set by senders of a transaction block
	 * It is an epoch reference that sets a deadline after which validators will no longer consider the transaction valid
	 * By default, there is no deadline for when a transaction must execute
	 */
	expiration?: Maybe<Epoch>;
	/**
	 * The gas input field provides information on what objects were used as gas
	 * As well as the owner of the gas object(s) and information on the gas price and budget
	 * If the owner of the gas object(s) is not the same as the sender,
	 * the transaction block is a sponsored transaction block.
	 */
	gasInput?: Maybe<GasInput>;
	kind?: Maybe<TransactionBlockKind>;
	/** The address of the user sending this transaction block */
	sender?: Maybe<Address>;
	/** A list of signatures of all signers, senders, and potentially the gas owner if this is a sponsored transaction. */
	signatures?: Maybe<Array<Maybe<TransactionSignature>>>;
};

export type TransactionBlockConnection = {
	__typename?: 'TransactionBlockConnection';
	/** A list of edges. */
	edges: Array<TransactionBlockEdge>;
	/** A list of nodes. */
	nodes: Array<TransactionBlock>;
	/** Information to aid in pagination. */
	pageInfo: PageInfo;
};

/** An edge in a connection. */
export type TransactionBlockEdge = {
	__typename?: 'TransactionBlockEdge';
	/** A cursor for use in pagination */
	cursor: Scalars['String']['output'];
	/** The item at the end of the edge */
	node: TransactionBlock;
};

export type TransactionBlockEffects = {
	__typename?: 'TransactionBlockEffects';
	balanceChanges?: Maybe<Array<Maybe<BalanceChange>>>;
	checkpoint?: Maybe<Checkpoint>;
	dependencies?: Maybe<Array<Maybe<TransactionBlock>>>;
	epoch?: Maybe<Epoch>;
	errors?: Maybe<Scalars['String']['output']>;
	gasEffects?: Maybe<GasEffects>;
	lamportVersion?: Maybe<Scalars['Int']['output']>;
	objectChanges?: Maybe<Array<Maybe<ObjectChange>>>;
	status: ExecutionStatus;
	transactionBlock?: Maybe<TransactionBlock>;
};

export type TransactionBlockFilter = {
	afterCheckpoint?: InputMaybe<Scalars['Int']['input']>;
	atCheckpoint?: InputMaybe<Scalars['Int']['input']>;
	beforeCheckpoint?: InputMaybe<Scalars['Int']['input']>;
	changedObject?: InputMaybe<Scalars['SuiAddress']['input']>;
	function?: InputMaybe<Scalars['String']['input']>;
	inputObject?: InputMaybe<Scalars['SuiAddress']['input']>;
	kind?: InputMaybe<TransactionBlockKindInput>;
	module?: InputMaybe<Scalars['String']['input']>;
	package?: InputMaybe<Scalars['SuiAddress']['input']>;
	paidAddress?: InputMaybe<Scalars['SuiAddress']['input']>;
	recvAddress?: InputMaybe<Scalars['SuiAddress']['input']>;
	sentAddress?: InputMaybe<Scalars['SuiAddress']['input']>;
	signAddress?: InputMaybe<Scalars['SuiAddress']['input']>;
	transactionIds?: InputMaybe<Array<Scalars['String']['input']>>;
};

export type TransactionBlockKind =
	| AuthenticatorStateUpdate
	| ChangeEpochTransaction
	| ConsensusCommitPrologueTransaction
	| EndOfEpochTransaction
	| GenesisTransaction
	| ProgrammableTransaction;

export enum TransactionBlockKindInput {
	ProgrammableTx = 'PROGRAMMABLE_TX',
	SystemTx = 'SYSTEM_TX',
}

export type TransactionSignature = {
	__typename?: 'TransactionSignature';
	base64Sig: Scalars['Base64']['output'];
};

/** Information about which previous versions of a package introduced its types. */
export type TypeOrigin = {
	__typename?: 'TypeOrigin';
	/** The storage ID of the package that first defined this type. */
	definingId: Scalars['SuiAddress']['output'];
	/** Module defining the type. */
	module: Scalars['String']['output'];
	/** Name of the struct. */
	struct: Scalars['String']['output'];
};

export type Validator = {
	__typename?: 'Validator';
	address: Address;
	atRisk?: Maybe<Scalars['Int']['output']>;
	commissionRate?: Maybe<Scalars['Int']['output']>;
	credentials?: Maybe<ValidatorCredentials>;
	description?: Maybe<Scalars['String']['output']>;
	exchangeRates?: Maybe<MoveObject>;
	exchangeRatesSize?: Maybe<Scalars['Int']['output']>;
	gasPrice?: Maybe<Scalars['BigInt']['output']>;
	imageUrl?: Maybe<Scalars['String']['output']>;
	name?: Maybe<Scalars['String']['output']>;
	nextEpochCommissionRate?: Maybe<Scalars['Int']['output']>;
	nextEpochCredentials?: Maybe<ValidatorCredentials>;
	nextEpochGasPrice?: Maybe<Scalars['BigInt']['output']>;
	nextEpochStake?: Maybe<Scalars['BigInt']['output']>;
	operationCap?: Maybe<MoveObject>;
	pendingPoolTokenWithdraw?: Maybe<Scalars['BigInt']['output']>;
	pendingStake?: Maybe<Scalars['BigInt']['output']>;
	pendingTotalSuiWithdraw?: Maybe<Scalars['BigInt']['output']>;
	poolTokenBalance?: Maybe<Scalars['BigInt']['output']>;
	projectUrl?: Maybe<Scalars['String']['output']>;
	reportRecords?: Maybe<Array<Scalars['SuiAddress']['output']>>;
	rewardsPool?: Maybe<Scalars['BigInt']['output']>;
	stakingPool?: Maybe<MoveObject>;
	stakingPoolActivationEpoch?: Maybe<Scalars['Int']['output']>;
	stakingPoolSuiBalance?: Maybe<Scalars['BigInt']['output']>;
	votingPower?: Maybe<Scalars['Int']['output']>;
};

export type ValidatorCredentials = {
	__typename?: 'ValidatorCredentials';
	netAddress?: Maybe<Scalars['String']['output']>;
	networkPubKey?: Maybe<Scalars['Base64']['output']>;
	p2PAddress?: Maybe<Scalars['String']['output']>;
	primaryAddress?: Maybe<Scalars['String']['output']>;
	proofOfPossession?: Maybe<Scalars['Base64']['output']>;
	protocolPubKey?: Maybe<Scalars['Base64']['output']>;
	workerAddress?: Maybe<Scalars['String']['output']>;
	workerPubKey?: Maybe<Scalars['Base64']['output']>;
};

/** Representation of `0x3::validator_set::ValidatorSet`. */
export type ValidatorSet = {
	__typename?: 'ValidatorSet';
	/** The current list of active validators. */
	activeValidators?: Maybe<Array<Validator>>;
	inactivePoolsSize?: Maybe<Scalars['Int']['output']>;
	pendingActiveValidatorsSize?: Maybe<Scalars['Int']['output']>;
	/**
	 * Validators that are pending removal from the active validator set, expressed as indices in
	 * to `activeValidators`.
	 */
	pendingRemovals?: Maybe<Array<Scalars['Int']['output']>>;
	stakePoolMappingsSize?: Maybe<Scalars['Int']['output']>;
	/** Total amount of stake for all active validators at the beginning of the epoch. */
	totalStake?: Maybe<Scalars['BigInt']['output']>;
	validatorCandidatesSize?: Maybe<Scalars['Int']['output']>;
};

export const Rpc_Credential_Fields = gql`
	fragment RPC_CREDENTIAL_FIELDS on ValidatorCredentials {
		netAddress
		networkPubKey
		p2PAddress
		primaryAddress
		workerPubKey
		workerAddress
		proofOfPossession
		protocolPubKey
	}
`;
export const Rpc_Validator_Fields = gql`
	fragment RPC_VALIDATOR_FIELDS on Validator {
		atRisk
		commissionRate
		exchangeRatesSize
		exchangeRates {
			asObject {
				location
			}
		}
		description
		gasPrice
		imageUrl
		name
		credentials {
			...RPC_CREDENTIAL_FIELDS
		}
		nextEpochCommissionRate
		nextEpochGasPrice
		nextEpochCredentials {
			...RPC_CREDENTIAL_FIELDS
		}
		nextEpochStake
		nextEpochCommissionRate
		operationCap {
			asObject {
				location
			}
		}
		pendingPoolTokenWithdraw
		pendingStake
		pendingTotalSuiWithdraw
		poolTokenBalance
		projectUrl
		rewardsPool
		stakingPoolSuiBalance
		address {
			location
		}
		votingPower
		reportRecords
	}
	${Rpc_Credential_Fields}
`;
export const Rpc_Object_Fields = gql`
	fragment RPC_OBJECT_FIELDS on Object {
		objectId: location
		bcs @include(if: $showBcs)
		version
		asMoveObject @include(if: $showType) {
			contents {
				type {
					signature
				}
			}
		}
		asMoveObject @include(if: $showContent) {
			contents {
				json
			}
		}
		asMoveObject @include(if: $showBcs) {
			hasPublicTransfer
			contents {
				type {
					signature
				}
			}
		}
		owner @include(if: $showOwner) {
			location
		}
		previousTransactionBlock @include(if: $showPreviousTransaction) {
			digest
		}
		storageRebate @include(if: $showStorageRebate)
		digest
		version
	}
`;
export const Rpc_Stake_Fields = gql`
	fragment RPC_STAKE_FIELDS on Stake {
		principal
		activeEpoch {
			epochId
		}
		requestEpoch {
			epochId
		}
		asMoveObject {
			contents {
				json
			}
			asObject {
				location
			}
		}
		estimatedReward
		activeEpoch {
			referenceGasPrice
		}
	}
`;
export const Rpc_Transaction_Fields = gql`
	fragment RPC_TRANSACTION_FIELDS on TransactionBlock {
		digest
		rawTransaction: bcs @include(if: $showRawInput)
		signatures {
			base64Sig
		}
		effects {
			checkpoint {
				digest
				sequenceNumber
			}
			balanceChanges @include(if: $showBalanceChanges) {
				owner {
					location
				}
				amount
			}
			dependencies @include(if: $showEffects) {
				digest
			}
			status @include(if: $showEffects)
			gasEffects @include(if: $showEffects) {
				gasSummary {
					storageCost
					storageRebate
					nonRefundableStorageFee
					computationCost
				}
			}
			executedEpoch: epoch @include(if: $showEffects) {
				epochId
			}
			objectChanges @include(if: $showObjectChanges) {
				idCreated
				idDeleted
				inputState {
					version
					digest
					objectId: location
					owner {
						location
					}
				}
				outputState {
					version
					digest
					objectId: location
					owner {
						location
					}
				}
			}
		}
	}
`;
export const GetAllBalances = gql`
	query getAllBalances($owner: SuiAddress!, $limit: Int, $cursor: String) {
		address(address: $owner) {
			balanceConnection(first: $limit, after: $cursor) {
				pageInfo {
					hasNextPage
					endCursor
				}
				nodes {
					coinType {
						signature
					}
					coinObjectCount
					totalBalance
				}
			}
		}
	}
`;
export const GetBalance = gql`
	query getBalance($owner: SuiAddress!, $type: String = "0x2::sui::SUI") {
		address(address: $owner) {
			balance(type: $type) {
				coinType {
					signature
				}
				coinObjectCount
				totalBalance
			}
		}
	}
`;
export const GetChainIdentifier = gql`
	query getChainIdentifier {
		chainIdentifier
	}
`;
export const GetCheckpoint = gql`
	query getCheckpoint($id: CheckpointId) {
		checkpoint(id: $id) {
			digest
			endOfEpoch {
				newCommittee {
					authorityName
					stakeUnit
				}
				nextProtocolVersion
			}
			epoch {
				epochId
			}
			rollingGasSummary {
				computationCost
				storageCost
				storageRebate
				nonRefundableStorageFee
			}
			networkTotalTransactions
			previousCheckpointDigest
			sequenceNumber
			timestamp
			transactionBlockConnection {
				nodes {
					digest
				}
			}
			validatorSignature
		}
	}
`;
export const GetCoins = gql`
	query getCoins(
		$owner: SuiAddress!
		$first: Int
		$cursor: String
		$type: String = "0x2::sui::SUI"
	) {
		address(address: $owner) {
			location
			coinConnection(first: $first, after: $cursor, type: $type) {
				pageInfo {
					hasNextPage
					endCursor
				}
				nodes {
					coinObjectId: id
					balance
					asMoveObject {
						contents {
							type {
								repr
							}
						}
						asObject {
							version
							digest
							previousTransactionBlock {
								digest
							}
						}
					}
				}
			}
		}
	}
`;
export const GetCurrentEpoch = gql`
	query getCurrentEpoch {
		epoch {
			epochId
			validatorSet {
				activeValidators {
					...RPC_VALIDATOR_FIELDS
				}
			}
			firstCheckpoint: checkpointConnection(first: 1) {
				nodes {
					digest
					sequenceNumber
				}
			}
			startTimestamp
			endTimestamp
			referenceGasPrice
		}
	}
	${Rpc_Validator_Fields}
`;
export const GetLatestCheckpointSequenceNumber = gql`
	query getLatestCheckpointSequenceNumber {
		checkpoint {
			sequenceNumber
		}
	}
`;
export const GetLatestSuiSystemState = gql`
	query getLatestSuiSystemState {
		latestSuiSystemState {
			referenceGasPrice
			safeMode {
				enabled
				gasSummary {
					computationCost
					nonRefundableStorageFee
					storageCost
					storageRebate
				}
			}
			stakeSubsidy {
				balance
				currentDistributionAmount
				decreaseRate
				distributionCounter
				periodLength
			}
			storageFund {
				nonRefundableBalance
				totalObjectStorageRebates
			}
			systemStateVersion
			systemParameters {
				minValidatorCount
				maxValidatorCount
				minValidatorJoiningStake
				durationMs
				validatorLowStakeThreshold
				validatorLowStakeGracePeriod
				validatorVeryLowStakeThreshold
			}
			protocolConfigs {
				protocolVersion
			}
			validatorSet {
				activeValidators {
					...RPC_VALIDATOR_FIELDS
				}
				inactivePoolsSize
				pendingActiveValidatorsSize
				validatorCandidatesSize
				pendingRemovals
				totalStake
			}
			epoch {
				epochId
				startTimestamp
				endTimestamp
			}
		}
	}
	${Rpc_Validator_Fields}
`;
export const GetMoveFunctionArgTypes = gql`
	query getMoveFunctionArgTypes($packageId: SuiAddress!, $module: String!) {
		object(address: $packageId) {
			asMovePackage {
				module(name: $module) {
					fileFormatVersion
				}
			}
		}
	}
`;
export const GetNormalizedMoveFunction = gql`
	query getNormalizedMoveFunction($packageId: SuiAddress!, $module: String!) {
		object(address: $packageId) {
			asMovePackage {
				module(name: $module) {
					fileFormatVersion
				}
			}
		}
	}
`;
export const GetNormalizedMoveModule = gql`
	query getNormalizedMoveModule($packageId: SuiAddress!, $module: String!) {
		object(address: $packageId) {
			asMovePackage {
				module(name: $module) {
					fileFormatVersion
				}
			}
		}
	}
`;
export const GetNormalizedMoveModulesByPackage = gql`
	query getNormalizedMoveModulesByPackage($packageId: SuiAddress!, $limit: Int, $cursor: String) {
		object(address: $packageId) {
			asMovePackage {
				moduleConnection(first: $limit, after: $cursor) {
					pageInfo {
						hasNextPage
						endCursor
					}
					nodes {
						fileFormatVersion
					}
				}
			}
		}
	}
`;
export const GetNormalizedMoveStruct = gql`
	query getNormalizedMoveStruct($packageId: SuiAddress!, $module: String!) {
		object(address: $packageId) {
			asMovePackage {
				module(name: $module) {
					fileFormatVersion
				}
			}
		}
	}
`;
export const GetProtocolConfig = gql`
	query getProtocolConfig($protocolVersion: Int) {
		protocolConfig(protocolVersion: $protocolVersion) {
			protocolVersion
			configs {
				key
				value
			}
			featureFlags {
				key
				value
			}
		}
	}
`;
export const GetReferenceGasPrice = gql`
	query getReferenceGasPrice {
		epoch {
			referenceGasPrice
		}
	}
`;
export const ResolveNameServiceAddress = gql`
	query resolveNameServiceAddress($name: String!) {
		resolveNameServiceAddress(name: $name) {
			location
		}
	}
`;
export const ResolveNameServiceNames = gql`
	query resolveNameServiceNames($address: SuiAddress!) {
		address(address: $address) {
			defaultNameServiceName
		}
	}
`;
export const GetOwnedObjects = gql`
	query getOwnedObjects(
		$owner: SuiAddress!
		$limit: Int
		$cursor: String
		$showBcs: Boolean = false
		$showContent: Boolean = false
		$showType: Boolean = false
		$showOwner: Boolean = false
		$showPreviousTransaction: Boolean = false
		$showStorageRebate: Boolean = false
		$filter: ObjectFilter
	) {
		address(address: $owner) {
			objectConnection(first: $limit, after: $cursor, filter: $filter) {
				pageInfo {
					hasNextPage
					endCursor
				}
				nodes {
					...RPC_OBJECT_FIELDS
				}
			}
		}
	}
	${Rpc_Object_Fields}
`;
export const GetObject = gql`
	query getObject(
		$id: SuiAddress!
		$showBcs: Boolean = false
		$showOwner: Boolean = false
		$showPreviousTransaction: Boolean = false
		$showContent: Boolean = false
		$showType: Boolean = false
		$showStorageRebate: Boolean = false
	) {
		object(address: $id) {
			...RPC_OBJECT_FIELDS
		}
	}
	${Rpc_Object_Fields}
`;
export const TryGetPastObject = gql`
	query tryGetPastObject(
		$id: SuiAddress!
		$version: Int
		$showBcs: Boolean = false
		$showOwner: Boolean = false
		$showPreviousTransaction: Boolean = false
		$showContent: Boolean = false
		$showType: Boolean = false
		$showStorageRebate: Boolean = false
	) {
		object(address: $id) {
			...RPC_OBJECT_FIELDS
		}
	}
	${Rpc_Object_Fields}
`;
export const MultiGetObjects = gql`
	query multiGetObjects(
		$ids: [SuiAddress!]!
		$limit: Int
		$cursor: String
		$showBcs: Boolean = false
		$showContent: Boolean = false
		$showType: Boolean = false
		$showOwner: Boolean = false
		$showPreviousTransaction: Boolean = false
		$showStorageRebate: Boolean = false
	) {
		objectConnection(first: $limit, after: $cursor, filter: { objectIds: $ids }) {
			pageInfo {
				hasNextPage
				endCursor
			}
			nodes {
				...RPC_OBJECT_FIELDS
			}
		}
	}
	${Rpc_Object_Fields}
`;
export const QueryEvents = gql`
	query queryEvents($filter: EventFilter!, $limit: Int, $cursor: String) {
		eventConnection(filter: $filter, first: $limit, after: $cursor) {
			pageInfo {
				hasNextPage
				endCursor
			}
			nodes {
				id
				sendingModuleId {
					package {
						asObject {
							location
						}
					}
					name
				}
				senders {
					location
				}
				eventType {
					repr
				}
				json
				bcs
				timestamp
			}
		}
	}
`;
export const GetStakes = gql`
	query getStakes($owner: SuiAddress!, $limit: Int, $cursor: String) {
		address(address: $owner) {
			stakeConnection(first: $limit, after: $cursor) {
				pageInfo {
					hasNextPage
					endCursor
				}
				nodes {
					...RPC_STAKE_FIELDS
				}
			}
		}
	}
	${Rpc_Stake_Fields}
`;
export const GetStakesByIds = gql`
	query getStakesByIds($ids: [SuiAddress!]!, $limit: Int, $cursor: String) {
		objectConnection(first: $limit, after: $cursor, filter: { objectIds: $ids }) {
			pageInfo {
				hasNextPage
				endCursor
			}
			nodes {
				asMoveObject {
					asStake {
						...RPC_STAKE_FIELDS
					}
				}
			}
		}
	}
	${Rpc_Stake_Fields}
`;
export const QueryTransactionBlocks = gql`
	query queryTransactionBlocks(
		$first: Int
		$last: Int
		$before: String
		$after: String
		$showBalanceChanges: Boolean = false
		$showEffects: Boolean = false
		$showObjectChanges: Boolean = false
		$showRawInput: Boolean = false
		$filter: TransactionBlockFilter
	) {
		transactionBlockConnection(
			first: $first
			after: $after
			last: $last
			before: $before
			filter: $filter
		) {
			pageInfo {
				hasNextPage
				endCursor
			}
			nodes {
				...RPC_TRANSACTION_FIELDS
			}
		}
	}
	${Rpc_Transaction_Fields}
`;
export const GetTransactionBlock = gql`
	query getTransactionBlock(
		$digest: String!
		$showBalanceChanges: Boolean = false
		$showEffects: Boolean = false
		$showObjectChanges: Boolean = false
		$showRawInput: Boolean = false
	) {
		transactionBlock(digest: $digest) {
			...RPC_TRANSACTION_FIELDS
		}
	}
	${Rpc_Transaction_Fields}
`;
export const MultiGetTransactionBlocks = gql`
	query multiGetTransactionBlocks(
		$digests: [String!]!
		$limit: Int
		$cursor: String
		$showBalanceChanges: Boolean = false
		$showEffects: Boolean = false
		$showObjectChanges: Boolean = false
		$showRawInput: Boolean = false
	) {
		transactionBlockConnection(
			first: $limit
			after: $cursor
			filter: { transactionIds: $digests }
		) {
			pageInfo {
				hasNextPage
				endCursor
			}
			nodes {
				...RPC_TRANSACTION_FIELDS
			}
		}
	}
	${Rpc_Transaction_Fields}
`;
