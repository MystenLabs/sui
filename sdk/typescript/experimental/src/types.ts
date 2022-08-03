// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Types and Interface definitions for the experimental SDK.
 *
 * These definitions provide the foundation for the TypeScript system which mimics
 * the underlying BCS structs.
 *
 * TransactionData is the type that is used for transaction sending.
 *
 * @module types
 */

import { bcs, decodeStr, encodeStr } from "@mysten/bcs";

// This buddy collects definitions for BCS.
const initializers: Function[] = [];

initializers.push((b: typeof bcs) =>
  b
    .registerVectorType("vector<u8>", "u8")
    .registerVectorType("vector<vector<u8>>", "vector<u8>")
    .registerAddressType("ObjectID", 20)
    .registerAddressType("SuiAddress", 20)
    .registerType(
      "utf8string",
      (writer, str) => {
        let bytes = Array.from(Buffer.from(str));
        return writer.writeVec(bytes, (writer, el) => writer.write8(el));
      },
      (reader) => {
        let bytes = reader.readVec((reader) => reader.read8());
        return Buffer.from(bytes).toString("utf-8");
      }
    )
    .registerType(
      "ObjectDigest",
      (writer, str) => {
        let bytes = Array.from(decodeStr(str, "base64"));
        return writer.writeVec(bytes, (writer, el) => writer.write8(el));
      },
      (reader) => {
        let bytes = reader.readVec((reader) => reader.read8());
        return encodeStr(new Uint8Array(bytes), "base64");
      }
    )
);

/**
 * Object Reference which is required for transaction building.
 */
export type SuiObjectRef = {
  objectId: string;
  version: number;
  digest: string;
};

initializers.push((b: typeof bcs) =>
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
};

initializers.push((b: typeof bcs) =>
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
};

initializers.push((b: typeof bcs) =>
  b.registerStructType("PublishTx", {
    modules: "vector<vector<u8>>",
  })
);

// ========== Move Call Tx ===========

/**
 * An object argument.
 */
export type ObjectArg = { ImmOrOwned: SuiObjectRef } | { Shared: string };

/**
 * An argument for the transaction. It is a 'meant' enum which expects to have
 * one of the optional properties. If not, the BCS error will be thrown while
 * attempting to form a transaction.
 *
 * Example:
 * ```js
 * let arg1: CallArg = { Object: { Shared: '5460cf92b5e3e7067aaace60d88324095fd22944' } };
 * let arg2: CallArg = { Pure: bcs.ser('u64', 100000) };
 * let arg3: CallArg = { Object: { ImmOrOwnedObject: {
 *   objectId: '4047d2e25211d87922b6650233bd0503a6734279',
 *   version: 1,
 *   digest: 'bCiANCht4O9MEUhuYjdRCqRPZjr2rJ8MfqNiwyhmRgA='
 * } } };
 * ```
 *
 * For `Pure` arguments BCS is required. You must encode the values with BCS according
 * to the type required by the called function. Pure accepts only serialized values
 */
export type CallArg = { Pure: Iterable<number> } | { Object: ObjectArg };

initializers.push((b: typeof bcs) =>
  b
    .registerEnumType("ObjectArg", {
      ImmOrOwned: "SuiObjectRef",
      Shared: "ObjectID",
    })
    .registerEnumType("CallArg", {
      Pure: "vector<u8>",
      Object: "ObjectArg",
    })
);

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
  | { struct: StructTag };

initializers.push((b: typeof bcs) =>
  b
    .registerEnumType("TypeTag", {
      bool: null,
      u8: null,
      u64: null,
      u128: null,
      address: null,
      signer: null,
      vector: "TypeTag",
      struct: "StructTag",
    })
    .registerVectorType("vector<TypeTag>", "TypeTag")
    .registerStructType("StructTag", {
      address: "SuiAddress",
      module: "string",
      name: "string",
      typeParams: "vector<TypeTag>",
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
};

initializers.push((b: typeof bcs) =>
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

initializers.push((b: typeof bcs) =>
  b.registerEnumType("Transaction", {
    TransferCoin: "TransferCoinTx",
    Publish: "PublishTx",
    Call: "MoveCallTx",
  })
);

/**
 * Transaction kind - either Batch or Single.
 *
 * Can be improved to change serialization automatically based on
 * the passed value (single Transaction or an array).
 */
export type TransactionKind =
  | { Single: Transaction }
  | { Batch: Transaction[] };

initializers.push((b: typeof bcs) =>
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
  gasPrice: number;
  kind: TransactionKind;
  gasPayment: SuiObjectRef;
};

initializers.push((b: typeof bcs) =>
  b.registerStructType("TransactionData", {
    kind: "TransactionKind",
    sender: "SuiAddress",
    gasPayment: "SuiObjectRef",
    gasPrice: "u64",
    gasBudget: "u64",
  })
);

/**
 * Initialize BCS definitions.
 * @param {BCS} bcs
 */
export function registerTypes(b: typeof bcs): typeof bcs {
  for (let init of initializers) {
    init(b);
  }

  return b;
}
