// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS, decodeStr, encodeStr, getSuiMoveConfig } from '@mysten/bcs';
import { SuiObjectRef } from './objects';

const bcs = new BCS(getSuiMoveConfig());

bcs
  .registerType(
    'utf8string',
    (writer, str) => {
      const bytes = Array.from(new TextEncoder().encode(str));
      return writer.writeVec(bytes, (writer, el) => writer.write8(el));
    },
    (reader) => {
      let bytes = reader.readVec((reader) => reader.read8());
      return new TextDecoder().decode(new Uint8Array(bytes));
    }
  )
  .registerType(
    'ObjectDigest',
    (writer, str) => {
      let bytes = Array.from(decodeStr(str, 'base64'));
      return writer.writeVec(bytes, (writer, el) => writer.write8(el));
    },
    (reader) => {
      let bytes = reader.readVec((reader) => reader.read8());
      return encodeStr(new Uint8Array(bytes), 'base64');
    }
  );

bcs.registerStructType('SuiObjectRef', {
  objectId: 'address',
  version: 'u64',
  digest: 'ObjectDigest',
});

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

bcs.registerStructType('TransferObjectTx', {
  recipient: 'address',
  object_ref: 'SuiObjectRef',
});

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

bcs
  .registerStructType('PayTx', {
    coins: 'vector<SuiObjectRef>',
    recipients: 'vector<address>',
    amounts: 'vector<u64>',
  });

bcs.registerStructType('PaySuiTx', {
  coins: 'vector<SuiObjectRef>',
  recipients: 'vector<address>',
  amounts: 'vector<u64>',
});

bcs.registerStructType('PayAllSuiTx', {
  coins: 'vector<SuiObjectRef>',
  recipient: 'address',
});

bcs.registerEnumType('Option<T>', {
  None: null,
  Some: 'T',
});

bcs.registerStructType('TransferSuiTx', {
  recipient: 'address',
  amount: 'Option<u64>',
});

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

bcs.registerStructType('PublishTx', {
  modules: 'vector<vector<u8>>',
});

// ========== Move Call Tx ===========

/**
 * A reference to a shared object.
 */
export type SharedObjectRef = {
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
  | { Shared: SharedObjectRef };

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
  | { Pure: ArrayLike<number> }
  | { Object: ObjectArg }
  | { ObjVec: ArrayLike<ObjectArg> };

bcs
  .registerStructType('SharedObjectRef', {
    objectId: 'address',
    initialSharedVersion: 'u64',
  })
  .registerEnumType('ObjectArg', {
    ImmOrOwned: 'SuiObjectRef',
    Shared: 'SharedObjectRef',
  })
  .registerEnumType('CallArg', {
    Pure: 'vector<u8>',
    Object: 'ObjectArg',
    ObjVec: 'vector<ObjectArg>',
  });

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
  | { u256: null }  ;

bcs
  .registerEnumType('TypeTag', {
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
  })
  .registerStructType('StructTag', {
    address: 'address',
    module: 'string',
    name: 'string',
    typeParams: 'vector<TypeTag>',
  });

/**
 * Transaction type used for calling Move modules' functions.
 * Should be crafted carefully, because the order of type parameters and
 * arguments matters.
 */
export type MoveCallTx = {
  Call: {
    package: SuiObjectRef;
    module: string;
    function: string;
    typeArguments: TypeTag[];
    arguments: CallArg[];
  };
};

bcs
  .registerStructType('MoveCallTx', {
    package: 'SuiObjectRef',
    module: 'string',
    function: 'string',
    typeArguments: 'vector<TypeTag>',
    arguments: 'vector<CallArg>',
  });

// ========== TransactionData ===========

export type Transaction =
  | MoveCallTx
  | PayTx
  | PaySuiTx
  | PayAllSuiTx
  | PublishTx
  | TransferObjectTx
  | TransferSuiTx;

bcs.registerEnumType('Transaction', {
  TransferObject: 'TransferObjectTx',
  Publish: 'PublishTx',
  Call: 'MoveCallTx',
  TransferSui: 'TransferSuiTx',
  Pay: 'PayTx',
  PaySui: 'PaySuiTx',
  PayAllSui: 'PayAllSuiTx',
});
/**
 * Transaction kind - either Batch or Single.
 *
 * Can be improved to change serialization automatically based on
 * the passed value (single Transaction or an array).
 */
export type TransactionKind =
  | { Single: Transaction }
  | { Batch: Transaction[] };

bcs
  .registerEnumType('TransactionKind', {
    Single: 'Transaction',
    Batch: 'vector<Transaction>',
  });

/**
 * The TransactionData to be signed and sent to the RPC service.
 *
 * Field `sender` is made optional as it can be added during the signing
 * process and there's no need to define it sooner.
 */
export type TransactionData = {
  sender?: string; //
  gasBudget: number;
  gasPrice: number;
  kind: TransactionKind;
  gasPayment: SuiObjectRef;
};

bcs.registerStructType('TransactionData', {
  kind: 'TransactionKind',
  sender: 'address',
  gasPayment: 'SuiObjectRef',
  gasPrice: 'u64',
  gasBudget: 'u64',
});

/**
 * Signed transaction data needed to generate transaction digest.
 */
bcs.registerStructType('SenderSignedData', {
  data: 'TransactionData',
  txSignature: 'vector<u8>',
});

export { bcs };
