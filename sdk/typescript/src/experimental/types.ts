// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Types and Interface definitions for the SDK.
 *
 * They provide the foundation for the TypeScript system which mimics
 * the underlying BCS structs.
 *
 * @module sui-types
 */

import { BCS, decodeStr, encodeStr } from "./bcs";

// This buddy collects definitions for BCS.
const initializers: Function[] = [];

initializers.push((b: typeof BCS) =>
  b
    .registerVectorType("vector<u8>", "u8")
    .registerVectorType("vector<vector<u8>>", "vector<u8>")
    .registerAddressType("ObjectID", 20)
    .registerAddressType("SuiAddress", 20)
    .registerType(
      "ObjectDigest",
      (writer, str) => {
        let bytes = Array.from(decodeStr(str, 'base64'));
        return writer.writeVec(bytes, (writer, el) => writer.write8(el));
      },
      (reader) => {
        let bytes = reader.readVec((reader) => reader.read8());
        return encodeStr(new Uint8Array(bytes), 'base64');
      }
    )
);

export type SuiObjectRef = {
  objectId: string;
  version: number;
  digest: string;
}

initializers.push((b: typeof BCS) =>
  b.registerStructType("SuiObjectRef", {
    objectId: "ObjectID",
    version: "u64",
    digest: "ObjectDigest",
  })
);

/**
 * Transaction type used for transfering Coin objects between accounts.
 * For this transaction to be executed, and `SuiObjectRef` should be queried
 * upfront and used as a parameter.
 */
export type TransferCoinTx = {
  TransferCoin: {
    recipient: string;
    object_ref: SuiObjectRef;
  };
}

initializers.push((b: typeof BCS) =>
  b.registerStructType("TransferCoinTx", {
    recipient: "SuiAddress",
    object_ref: "SuiObjectRef",
  })
);

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
    modules: Iterable<Iterable<number>>;
  };
}

initializers.push((b: typeof BCS) =>
  b.registerStructType("PublishTx", {
    modules: "vector<vector<u8>>",
  })
);

// ========== Move Call Tx ===========

/**
 * An argument for the transaction. It is a 'meant' enum which expects to have
 * one of the optional properties. If not, the BCS error will be thrown while
 * attempting to form a transaction.
 *
 * Example:
 * ```js
 * let arg1: CallArg = { SharedObject: '5460cf92b5e3e7067aaace60d88324095fd22944' };
 * let arg2: CallArg = { Pure: bcs.ser('u64', 100000) };
 * let arg3: CallArg = { ImmOrOwnedObject: {
 *   ObjectID: '4047d2e25211d87922b6650233bd0503a6734279',
 *   SequenceNumber: 1,
 *   ObjectDigest: Buffer.from('bCiANCht4O9MEUhuYjdRCqRPZjr2rJ8MfqNiwyhmRgA=', 'base64')
 * } };
 * ```
 *
 * For `Pure` arguments BCS is required. You must encode the values with BCS according
 * to the type required by the called function. Pure accepts only serialized values
 */
export type CallArg =
  | { SharedObject: string }
  | { Pure: Iterable<number> }
  | { ImmOrOwnedObject: SuiObjectRef }

initializers.push((b: typeof BCS) =>
  b.registerEnumType("CallArg", {
    Pure: "vector<u8>",
    ImmOrOwnedObject: "SuiObjectRef",
    SharedObject: "ObjectID",
  })
);

export type StructTag = {
  address: string,
  module: string,
  name: string,
  typeParams: TypeTag[]
};

export type TypeTag =
  | { bool: null }
  | { u8: null }
  | { u64: null }
  | { u128: null }
  | { address: null }
  | { signer: null }
  | { vector: TypeTag }
  | { struct: StructTag }

initializers.push((b: typeof BCS) => b
  .registerEnumType('TypeTag', {
    bool: null,
    u8: null,
    u64: null,
    u128: null,
    address: null,
    signer: null,
    vector: 'TypeTag',
    struct: 'StructTag'
  })
  .registerVectorType('vector<TypeTag>', 'TypeTag')
  .registerStructType('StructTag', {
    address: 'string',
    module: 'string',
    name: 'string',
    typeParams: 'vector<TypeTag>'
  })
);

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
}

initializers.push((b: typeof BCS) =>
  b
    .registerVectorType("vector<CallArg>", "CallArg")
    .registerStructType("MoveCallTx", {
      package: "SuiObjectRef",
      module: "string",
      function: "string",
      typeArguments: "vector<TypeTag>",
      arguments: "vector<CallArg>",
    })
);

// ========== TransactionData ===========

export type Transaction = MoveCallTx | PublishTx | TransferCoinTx;

initializers.push((b: typeof BCS) =>
  b.registerEnumType("Transaction", {
    TransferCoin: "TransferCoinTx",
    Publish: "PublishTx",
    Call: "MoveCallTx",
  })
);

export type TransactionKind =
  | { Single: Transaction }
  | { Batch: Transaction[] };

initializers.push((b: typeof BCS) =>
  b
    .registerVectorType("vector<Transaction>", "Transaction")
    .registerEnumType("TransactionKind", {
      Single: "Transaction",
      Batch: "vector<Transaction>",
    })
);

/**
 * The TransactionData to be signed and sent to the Gateway service.
 *
 * Field `sender` is made optional as it can be added during the signing
 * process and there's no need to define it sooner.
 */
export type TransactionData = {
  sender?: string; //
  gasBudget: number;
  kind: TransactionKind;
  gasPayment: SuiObjectRef;
}

initializers.push((b: typeof BCS) =>
  b.registerStructType("TransactionData", {
    kind: "TransactionKind",
    sender: "SuiAddress",
    gasPayment: "SuiObjectRef",
    gasBudget: "u64",
  })
);

/**
 * Initialize BCS definitions.
 * @param {BCS} bcs
 */
export function registerTypes(bcs: typeof BCS) {
  for (let init of initializers) {
    init(bcs);
  }
}
