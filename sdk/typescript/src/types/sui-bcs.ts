// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  BCS,
  EnumTypeDefinition,
  getSuiMoveConfig,
  StructTypeDefinition,
} from '@mysten/bcs';
import { SuiObjectRef } from './objects';
import { RpcApiVersion } from './version';

function registerUTF8String(bcs: BCS) {
  bcs.registerType(
    'utf8string',
    (writer, str) => {
      const bytes = Array.from(new TextEncoder().encode(str));
      return writer.writeVec(bytes, (writer, el) => writer.write8(el));
    },
    (reader) => {
      let bytes = reader.readVec((reader) => reader.read8());
      return new TextDecoder().decode(new Uint8Array(bytes));
    },
  );
}

/**
 * Transaction type used for transferring objects.
 * For this transaction to be executed, and `SuiObjectRef` should be queried
 * upfront and used as a parameter.
 */
export type TransferObjectTx = {
  TransferObject: {
    recipient: string;
    object_ref: SuiObjectRef;
  };
};

/**
 * Transaction type used for transferring Sui.
 */
export type TransferSuiTx = {
  TransferSui: {
    recipient: string;
    amount: { Some: number } | { None: null };
  };
};

/**
 * Transaction type used for Pay transaction.
 */
export type PayTx = {
  Pay: {
    coins: SuiObjectRef[];
    recipients: string[];
    amounts: number[];
  };
};

export type PaySuiTx = {
  PaySui: {
    coins: SuiObjectRef[];
    recipients: string[];
    amounts: number[];
  };
};

export type PayAllSuiTx = {
  PayAllSui: {
    coins: SuiObjectRef[];
    recipient: string;
  };
};

/**
 * Transaction type used for publishing Move modules to the Sui.
 * Should be already compiled using `sui-move`, example:
 * ```
 * $ sui-move build
 * $ cat build/project_name/bytecode_modules/module.mv
 * ```
 * In JS:
 * ```
 * let file = fs.readFileSync('./move/build/project_name/bytecode_modules/module.mv');
 * let bytes = Array.from(bytes);
 * let modules = [ bytes ];
 *
 * // ... publish logic ...
 * ```
 *
 * Each module should be represented as a sequence of bytes.
 */
export type PublishTx = {
  Publish: {
    modules: ArrayLike<ArrayLike<number>>;
  };
};

// ========== Move Call Tx ===========

/**
 * A reference to a shared object.
 */
export type SharedObjectRef = {
  /** Hex code as string representing the object id */
  objectId: string;

  /** The version the object was shared at */
  initialSharedVersion: number;

  /** Whether reference is mutable */
  mutable: boolean;
};

/**
 * A reference to a shared object from 0.23.0.
 */
export type SharedObjectRef_23 = {
  /** Hex code as string representing the object id */
  objectId: string;

  /** The version the object was shared at */
  initialSharedVersion: number;
};

/**
 * An object argument.
 */
export type ObjectArg =
  | { ImmOrOwned: SuiObjectRef }
  | { Shared: SharedObjectRef | SharedObjectRef_23 };

/**
 * A pure argument.
 */
export type PureArg = { Pure: ArrayLike<number> };

export function isPureArg(arg: any): arg is PureArg {
  return (arg as PureArg).Pure !== undefined;
}

/**
 * An argument for the transaction. It is a 'meant' enum which expects to have
 * one of the optional properties. If not, the BCS error will be thrown while
 * attempting to form a transaction.
 *
 * Example:
 * ```js
 * let arg1: CallArg = { Object: { Shared: {
 *   objectId: '5460cf92b5e3e7067aaace60d88324095fd22944',
 *   initialSharedVersion: 1,
 * } } };
 * let arg2: CallArg = { Pure: bcs.set(bcs.STRING, 100000).toBytes() };
 * let arg3: CallArg = { Object: { ImmOrOwned: {
 *   objectId: '4047d2e25211d87922b6650233bd0503a6734279',
 *   version: 1,
 *   digest: 'bCiANCht4O9MEUhuYjdRCqRPZjr2rJ8MfqNiwyhmRgA='
 * } } };
 * ```
 *
 * For `Pure` arguments BCS is required. You must encode the values with BCS according
 * to the type required by the called function. Pure accepts only serialized values
 */
export type CallArg =
  | PureArg
  | { Object: ObjectArg }
  | { ObjVec: ArrayLike<ObjectArg> };

/**
 * Kind of a TypeTag which is represented by a Move type identifier.
 */
export type StructTag = {
  address: string;
  module: string;
  name: string;
  typeParams: TypeTag[];
};

/**
 * Sui TypeTag object. A decoupled `0x...::module::Type<???>` parameter.
 */
export type TypeTag =
  | { bool: null }
  | { u8: null }
  | { u64: null }
  | { u128: null }
  | { address: null }
  | { signer: null }
  | { vector: TypeTag }
  | { struct: StructTag }
  | { u16: null }
  | { u32: null }
  | { u256: null };

/**
 * Transaction type used for calling Move modules' functions.
 * Should be crafted carefully, because the order of type parameters and
 * arguments matters.
 */
export type MoveCallTx = {
  Call: {
    package: string;
    module: string;
    function: string;
    typeArguments: TypeTag[];
    arguments: CallArg[];
  };
};

// ========== TransactionData ===========

export type SingleTransactionKind =
  | MoveCallTx
  | PayTx
  | PaySuiTx
  | PayAllSuiTx
  | PublishTx
  | TransferObjectTx
  | TransferSuiTx;

/**
 * Transaction kind - either Batch or Single.
 *
 * Can be improved to change serialization automatically based on
 * the passed value (single Transaction or an array).
 */
export type TransactionKind =
  | { Single: SingleTransactionKind }
  | { Batch: SingleTransactionKind[] };

/**
 * The GasData to be used in the transaction.
 */
export type GasData = {
  payment: SuiObjectRef[];
  owner: string; // Gas Object's owner
  price: number;
  budget: number;
};

/**
 * TransactionExpiration
 *
 * Indications the expiration time for a transaction.
 */
export type TransactionExpiration = { None: null } | { Epoch: number };

/**
 * The TransactionData to be signed and sent to the RPC service.
 *
 * Field `sender` is made optional as it can be added during the signing
 * process and there's no need to define it sooner.
 *
 * Field `expiration` is made optional as it is defaulted to `None`.
 */
export type TransactionData = {
  messageVersion: 1; // Eventually: 1 | 2 | ...
  sender?: string;
  kind: TransactionKind;
  gasData: GasData;
  expiration?: TransactionExpiration;
};

export type TransactionDataBCS = {
  V1: Omit<TransactionData, 'messageVersion'>;
};

export const TRANSACTION_DATA_TYPE_TAG = Array.from('TransactionData::').map(
  (e) => e.charCodeAt(0),
);

export function deserializeTransactionBytesToTransactionData(
  bcs: BCS,
  bytes: Uint8Array,
): TransactionData {
  let deserialized = bcs.de('TransactionData', bytes);
  let inner, messageVersion;

  if (deserialized.V1 != null) {
    inner = deserialized.V1;
    messageVersion = 1;
  } else {
    throw new Error(`Unknown message: ${JSON.stringify(deserialized)}`);
  }

  return {
    messageVersion,
    ...inner,
  };
}

// Move name of the Vector type.
const VECTOR = 'vector';

// Imported to explicitly tell typescript that types match
type TypeSchema = {
  structs?: { [key: string]: StructTypeDefinition };
  enums?: { [key: string]: EnumTypeDefinition };
  aliases?: { [key: string]: string };
};

const TransactionDataV1 = {
  kind: 'TransactionKind',
  sender: BCS.ADDRESS,
  gasData: 'GasData',
  expiration: 'TransactionExpiration',
};

const BCS_SPEC: TypeSchema = {
  enums: {
    'Option<T>': {
      None: null,
      Some: 'T',
    },
    ObjectArg: {
      ImmOrOwned: 'SuiObjectRef',
      Shared: 'SharedObjectRef',
    },
    CallArg: {
      Pure: [VECTOR, BCS.U8],
      Object: 'ObjectArg',
      ObjVec: [VECTOR, 'ObjectArg'],
    },
    TypeTag: {
      bool: null,
      u8: null,
      u64: null,
      u128: null,
      address: null,
      signer: null,
      vector: 'TypeTag',
      struct: 'StructTag',
      u16: null,
      u32: null,
      u256: null,
    },
    Transaction: {
      TransferObject: 'TransferObjectTx',
      Publish: 'PublishTx',
      Call: 'MoveCallTx',
      TransferSui: 'TransferSuiTx',
      Pay: 'PayTx',
      PaySui: 'PaySuiTx',
      PayAllSui: 'PayAllSuiTx',

      // can not be called from sui.js; dummy placement
      // to set the enum counter right for ProgrammableTransact
      ChangeEpoch: null,
      Genesis: null,
      ConsensusCommitPrologue: null,

      ProgrammableTransaction: 'ProgrammableTransaction',
    },
    TransactionKind: {
      Single: 'Transaction',
      Batch: [VECTOR, 'Transaction'],
    },
    TransactionExpiration: {
      None: null,
      Epoch: BCS.U64,
    },
    TransactionData: {
      V1: 'TransactionDataV1',
    },
  },
  structs: {
    SuiObjectRef: {
      objectId: BCS.ADDRESS,
      version: BCS.U64,
      digest: 'ObjectDigest',
    },
    TransferObjectTx: {
      recipient: BCS.ADDRESS,
      object_ref: 'SuiObjectRef',
    },
    PayTx: {
      coins: [VECTOR, 'SuiObjectRef'],
      recipients: [VECTOR, BCS.ADDRESS],
      amounts: [VECTOR, BCS.U64],
    },
    PaySuiTx: {
      coins: [VECTOR, 'SuiObjectRef'],
      recipients: [VECTOR, BCS.ADDRESS],
      amounts: [VECTOR, BCS.U64],
    },
    PayAllSuiTx: {
      coins: [VECTOR, 'SuiObjectRef'],
      recipient: BCS.ADDRESS,
    },
    TransferSuiTx: {
      recipient: BCS.ADDRESS,
      amount: ['Option', BCS.U64],
    },
    PublishTx: {
      modules: [VECTOR, [VECTOR, BCS.U8]],
    },
    SharedObjectRef: {
      objectId: BCS.ADDRESS,
      initialSharedVersion: BCS.U64,
      mutable: BCS.BOOL,
    },
    StructTag: {
      address: BCS.ADDRESS,
      module: BCS.STRING,
      name: BCS.STRING,
      typeParams: [VECTOR, 'TypeTag'],
    },
    MoveCallTx: {
      package: BCS.ADDRESS,
      module: BCS.STRING,
      function: BCS.STRING,
      typeArguments: [VECTOR, 'TypeTag'],
      arguments: [VECTOR, 'CallArg'],
    },
    GasData: {
      payment: [VECTOR, 'SuiObjectRef'],
      owner: BCS.ADDRESS,
      price: BCS.U64,
      budget: BCS.U64,
    },
    // Signed transaction data needed to generate transaction digest.
    SenderSignedData: {
      data: 'TransactionData',
      txSignatures: [VECTOR, [VECTOR, BCS.U8]],
    },
    TransactionDataV1,
  },
  aliases: {
    ObjectDigest: BCS.BASE58,
  },
};

// for version <= 0.27.0
const BCS_0_27_SPEC: TypeSchema = {
  structs: {
    ...BCS_SPEC.structs,
    TransactionData: {
      kind: 'TransactionKind',
      sender: BCS.ADDRESS,
      gasData: 'GasData',
    },
    SenderSignedData: {
      data: 'TransactionData',
      txSignature: [VECTOR, BCS.U8],
    },
  },
  enums: BCS_SPEC.enums,
  aliases: {
    ObjectDigest: BCS.BASE64,
  },
};

const bcs = new BCS({ ...getSuiMoveConfig(), types: BCS_SPEC });
registerUTF8String(bcs);

// ========== Backward Compatibility ===========
const bcs_0_27 = new BCS({ ...getSuiMoveConfig(), types: BCS_0_27_SPEC });
registerUTF8String(bcs_0_27);

export function bcsForVersion(v?: RpcApiVersion) {
  if (v?.major === 0 && v?.minor <= 27) {
    return bcs_0_27;
  }

  return bcs;
}

export { bcs };
