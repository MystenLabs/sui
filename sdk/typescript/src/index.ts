// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromExportedKeypair } from './cryptography/utils.js';
import { ECMHLiveObjectSetDigest, ExecutionDigests } from './types/checkpoints.js';
import { MIST_PER_SUI, SUI_DECIMALS } from './types/objects.js';
import {
	AuthorityQuorumSignInfo,
	GenericAuthoritySignature,
	SuiTransactionBlockKind,
} from './types/transactions.js';

// all exports deprecated, non-deprecated imports exported separately below
export * from './types/index.js';

export {
	/** @deprecated Import from `@mysten/sui.js/keypairs/ed5519` instead */
	type Ed25519KeypairData,
	/** @deprecated Import from `@mysten/sui.js/keypairs/ed5519` instead */
	Ed25519Keypair,
	/** @deprecated Import from `@mysten/sui.js/keypairs/ed5519` instead */
	Ed25519PublicKey,
} from './keypairs/ed25519/index.js';
export {
	/** @deprecated Import from `@mysten/sui.js/keypairs/secp256k1` instead */
	DEFAULT_SECP256K1_DERIVATION_PATH,
	/** @deprecated Import from `@mysten/sui.js/keypairs/secp256k1` instead */
	type Secp256k1KeypairData,
	/** @deprecated Import from `@mysten/sui.js/keypairs/secp256k1` instead */
	Secp256k1Keypair,
	/** @deprecated Import from `@mysten/sui.js/keypairs/secp256k1` instead */
	Secp256k1PublicKey,
} from './keypairs/secp256k1/index.js';
export {
	/** @deprecated Import from `@mysten/sui.js/keypairs/secp256r1` instead */
	DEFAULT_SECP256R1_DERIVATION_PATH,
	/** @deprecated Import from `@mysten/sui.js/keypairs/secp256r1` instead */
	type Secp256r1KeypairData,
	/** @deprecated Import from `@mysten/sui.js/keypairs/secp256r1` instead */
	Secp256r1Keypair,
	/** @deprecated Import from `@mysten/sui.js/keypairs/secp256k1` instead */
	Secp256r1PublicKey,
} from './keypairs/secp256r1/index.js';
export {
	/** @deprecated Signing methods are now available on the KeyPair classes */
	BaseSigner,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	type ExportedKeypair,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	Keypair,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	LEGACY_PRIVATE_KEY_SIZE,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	PRIVATE_KEY_SIZE,
} from './cryptography/keypair.js';
export {
	/** @deprecated Use the MultiSigPublicKey class from `@mysten/sui.js/multisig` instead */
	type CompressedSignature,
	/** @deprecated Use the MultiSigPublicKey class from `@mysten/sui.js/multisig` instead */
	MAX_SIGNER_IN_MULTISIG,
	/** @deprecated Use the MultiSigStruct from `@mysten/sui.js/multisig` instead */
	type MultiSig,
	/** @deprecated Use the MultiSigPublicKey class from `@mysten/sui.js/multisig` instead */
	type MultiSigPublicKey,
	/** @deprecated Use the MultiSigPublicKey class from `@mysten/sui.js/multisig` instead */
	type PubkeyEnumWeightPair,
	/** @deprecated Use the MultiSigPublicKey class from `@mysten/sui.js/multisig` instead */
	type PubkeyWeightPair,
	/** @deprecated Use the MultiSigPublicKey class from `@mysten/sui.js/multisig` instead */
	type PublicKeyEnum,
	/** @deprecated Use the MultiSigPublicKey class from `@mysten/sui.js/multisig` instead */
	combinePartialSigs,
	/** @deprecated Use the parseSerializedSignature from `@mysten/sui.js/cryptography` instead */
	decodeMultiSig,
	/** @deprecated Use the MultiSigPublicKey class from `@mysten/sui.js/multisig` instead */
	toMultiSigAddress,
} from './cryptography/multisig.js';

export {
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	PublicKey,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	type PublicKeyInitData,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	bytesEqual,
} from './cryptography/publickey.js';
export {
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	isValidBIP32Path,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	isValidHardenedPath,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	mnemonicToSeed,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	mnemonicToSeedHex,
} from './cryptography/mnemonics.js';

export {
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	SIGNATURE_FLAG_TO_SCHEME,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	SIGNATURE_SCHEME_TO_FLAG,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	SIGNATURE_SCHEME_TO_SIZE,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	type SerializeSignatureInput,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	type SerializedSignature,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	type SignatureFlag,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	type SignatureScheme,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	parseSerializedSignature,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	toSerializedSignature,
} from './cryptography/signature.js';

export {
	/** @deprecated This type will be removed because it cant accurately represent parsed signatures */
	type SignaturePubkeyPair,
	/** @deprecated use `publicKeyFromRawBytes` from `@mysten/sui.j/verify` instead */
	publicKeyFromSerialized,
	/** @deprecated use `parseSerializedSignature` from `@mysten/sui.j/cryptography` instead */
	toParsedSignaturePubkeyPair,
	/** @deprecated use `parseSerializedSignature` from `@mysten/sui.j/cryptography` instead */
	toSingleSignaturePubkeyPair,
} from './cryptography/utils.js';
export {
	/** @deprecated Use `SuiClient` from `@mysten/sui.js/client` instead */
	JsonRpcProvider,
	/** @deprecated Import from `@mysten/sui.js/client` instead */
	type OrderArguments,
	/** @deprecated Import from `@mysten/sui.js/client` instead */
	type PaginationArguments,
	/** @deprecated Use `SuiClientOptions` from `@mysten/sui.js/client` instead */
	type RpcProviderOptions,
} from './providers/json-rpc-provider.js';

export {
	/** @deprecated Import from `@mysten/sui.js/client` instead */
	type HttpHeaders,
	/** @deprecated This client will not be exported in the future */
	JsonRpcClient,
} from './rpc/client.js';

export {
	/** @deprecated Use `getFullnodeUrl` from `@mysten/sui.js/client` instead */
	Connection,
	/** @deprecated Use `getFullnodeUrl` from `@mysten/sui.js/client` instead */
	devnetConnection,
	/** @deprecated Use `getFullnodeUrl` from `@mysten/sui.js/client` instead */
	localnetConnection,
	/** @deprecated Use `getFullnodeUrl` from `@mysten/sui.js/client` instead */
	mainnetConnection,
	/** @deprecated Use `getFullnodeUrl` from `@mysten/sui.js/client` instead */
	testnetConnection,
} from './rpc/connection.js';

export {
	/** @deprecated This will not be exported from future version of this package */
	TypeTagSerializer,
} from './builder/type-tag-serializer.js';

export {
	/** @deprecated Use KeyPair classes from `@mysten/sui.js/keypairs/*` instead */
	type Signer,
} from './signers/signer.js';
export {
	/** @deprecated Use KeyPair classes from `@mysten/sui.js/keypairs/*` instead */
	RawSigner,
} from './signers/raw-signer.js';
export {
	/** @deprecated Use KeyPair classes from `@mysten/sui.js/keypairs/*` instead */
	SignerWithProvider,
} from './signers/signer-with-provider.js';

export {
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	AppId,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	type Intent,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	IntentScope,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	IntentVersion,
	/** @deprecated Import from `@mysten/sui.js/cryptography` instead */
	messageWithIntent,
} from './cryptography/intent.js';

export {
	/** @deprecated Use verify methods on PublicKey classes from `@mysten/sui.js/keypairs/*` instead */
	verifyMessage,
} from './utils/verify.js';

export {
	/** @deprecated Import from `@mysten/sui.js/client` instead */
	RPCValidationError,
} from './rpc/errors.js';

export {
	/** @deprecated Import from @mysten/sui.js/utils instead */
	fromB64,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	toB64,
} from '@mysten/bcs';

export {
	/** @deprecated Import from @mysten/sui.js/utils instead */
	SUI_ADDRESS_LENGTH,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	isValidSuiAddress,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	isValidSuiObjectId,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	isValidTransactionDigest,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	normalizeStructTag,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	normalizeSuiAddress,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	normalizeSuiObjectId,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	parseStructTag,
} from './utils/sui-types.js';

export {
	/** @deprecated Import from @mysten/sui.js/utils instead */
	formatAddress,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	formatDigest,
} from './utils/format.js';

export {
	/** @deprecated Import from @mysten/sui.js/utils instead */
	is,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	assert,
} from 'superstruct';

export {
	/** @deprecated this will be removed in future versions of the SDK */
	DEFAULT_CLIENT_OPTIONS,
	/** @deprecated this will be removed in future versions of the SDK */
	WebsocketClient,
	/** @deprecated this will be removed in future versions of the SDK */
	type WebsocketClientOptions,
	/** @deprecated this will be removed in future versions of the SDK */
	getWebsocketUrl,
} from './rpc/websocket-client.js';

export {
	/** @deprecated messages are now signed through the KeyPair classes */
	type SignedMessage,
	/**
	 * @deprecated transactions are now signed through the KeyPair classes
	 * or signed and executed directly using the SuiClient
	 */
	type SignedTransaction,
} from './signers/types.js';

export {
	/** @deprecated Import from `@mysten/sui.js/transactions` instead */
	builder,
	/** @deprecated Import from `@mysten/sui.js/transactions` instead */
	Transactions,
	/** @deprecated Import from `@mysten/sui.js/transactions` instead */
	Inputs,
	/** @deprecated Import from `@mysten/sui.js/transactions` instead */
	TransactionBlock,
	/** @deprecated Import from `@mysten/sui.js/transactions` instead */
	TransactionArgument,

	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	ARGUMENT,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	ARGUMENT_INNER,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	BuilderCallArg,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	CALL_ARG,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	COMPRESSED_SIGNATURE,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	ENUM_KIND,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	MULTISIG,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	MULTISIG_PK_MAP,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	MULTISIG_PUBLIC_KEY,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	MakeMoveVecTransaction,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	MergeCoinsTransaction,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	MoveCallTransaction,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	OBJECT_ARG,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	OPTION,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	ObjectCallArg,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	ObjectTransactionArgument,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	type Option,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PROGRAMMABLE_CALL,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PROGRAMMABLE_CALL_INNER,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PROGRAMMABLE_TX_BLOCK,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PUBLIC_KEY,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PublishTransaction,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PureCallArg,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PureTransactionArgument,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	SplitCoinsTransaction,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	TRANSACTION,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	TRANSACTION_INNER,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	TYPE_TAG,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	TransactionBlockInput,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	TransactionType,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	TransferObjectsTransaction,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	UpgradePolicy,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	UpgradeTransaction,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	VECTOR,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	getIdFromCallArg,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	getPureSerializationType,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	getSharedObjectInput,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	getTransactionType,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	isMutableSharedObjectInput,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	isSharedObjectInput,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	isTxContext,
} from './builder/index.js';

export {
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	ADD_STAKE_FUN_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	ADD_STAKE_LOCKED_COIN_FUN_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	COIN_TYPE_ARG_REGEX,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	Coin,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	type CoinMetadata,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	CoinMetadataStruct,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	Delegation,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	type DelegationData,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	type DelegationSuiObject,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	ID_STRUCT_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	OBJECT_MODULE_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PAY_JOIN_COIN_FUNC_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PAY_MODULE_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	PAY_SPLIT_COIN_VEC_FUNC_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	SuiSystemStateUtil,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	UID_STRUCT_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	VALIDATORS_EVENTS_QUERY,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	WITHDRAW_STAKE_FUN_NAME,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	isObjectDataFull,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	SUI_CLOCK_OBJECT_ID,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	SUI_FRAMEWORK_ADDRESS,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	SUI_SYSTEM_ADDRESS,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	SUI_SYSTEM_MODULE_NAME,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	SUI_SYSTEM_STATE_OBJECT_ID,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	SUI_TYPE_ARG,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	MOVE_STDLIB_ADDRESS,
} from './framework/index.js';

export {
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	type CallArg,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	type GasData,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	type ObjectArg,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	type PureArg,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	type SharedObjectRef,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	type StructTag,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	type TransactionExpiration,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	type TypeTag,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	bcs,
	/** @deprecated Import from @mysten/sui.js/bcs instead */
	isPureArg,
} from './bcs/index.js';

export {
	/** @deprecated Import from @mysten/sui.js/utils instead */
	MIST_PER_SUI,
	/** @deprecated Import from @mysten/sui.js/utils instead */
	SUI_DECIMALS,
	/** @deprecated this will be removed in future versions of the SDK */
	ECMHLiveObjectSetDigest,
	/** @deprecated this will be removed in future versions of the SDK */
	ExecutionDigests,
	/** @deprecated this will be removed in future versions of the SDK */
	GenericAuthoritySignature,
	/** @deprecated this will be removed in future versions of the SDK */
	SuiTransactionBlockKind,
	/** @deprecated this will be removed in future versions of the SDK */
	AuthorityQuorumSignInfo,
	/**
	 * @deprecated This will be removed in a future of version of the SDK.
	 * If this export is needed, please report your use case here:
	 * https://github.com/MystenLabs/sui/discussions/13150
	 */
	fromExportedKeypair,
};
