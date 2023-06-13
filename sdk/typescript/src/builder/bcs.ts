// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS, TypeName } from '@mysten/bcs';
import { bcs } from '../types/sui-bcs';
import { normalizeSuiAddress, TypeTag } from '../types';
import { TypeTagSerializer } from '../signers/txn-data-serializers/type-tag-serializer';
import { TransactionArgument, MoveCallTransaction } from './Transactions';

export const ARGUMENT_INNER = 'Argument';
export const VECTOR = 'vector';
export const OPTION = 'Option';
export const CALL_ARG = 'CallArg';
export const TYPE_TAG = 'TypeTag';
export const OBJECT_ARG = 'ObjectArg';
export const PROGRAMMABLE_TX_BLOCK = 'ProgrammableTransaction';
export const PROGRAMMABLE_CALL_INNER = 'ProgrammableMoveCall';
export const TRANSACTION_INNER = 'Transaction';
export const COMPRESSED_SIGNATURE = 'CompressedSignature';
export const PUBLIC_KEY = 'PublicKey';
export const MULTISIG_PUBLIC_KEY = 'MultiSigPublicKey';
export const MULTISIG_PK_MAP = 'MultiSigPkMap';
export const MULTISIG = 'MultiSig';

export const ENUM_KIND = 'EnumKind';

/** Wrapper around transaction Enum to support `kind` matching in TS */
export const TRANSACTION: TypeName = [ENUM_KIND, TRANSACTION_INNER];
/** Wrapper around Argument Enum to support `kind` matching in TS */
export const ARGUMENT: TypeName = [ENUM_KIND, ARGUMENT_INNER];

/** Custom serializer for decoding package, module, function easier */
export const PROGRAMMABLE_CALL = 'SimpleProgrammableMoveCall';

/** Transaction types */

export type Option<T> = { some: T } | { none: true };

export const builder = new BCS(bcs);
registerFixedArray(builder, 'FixedArray[64]', 64);
registerFixedArray(builder, 'FixedArray[33]', 33);
registerFixedArray(builder, 'FixedArray[32]', 32);

builder
  .registerStructType(PROGRAMMABLE_TX_BLOCK, {
    inputs: [VECTOR, CALL_ARG],
    transactions: [VECTOR, TRANSACTION],
  })
  .registerEnumType(ARGUMENT_INNER, {
    GasCoin: null,
    Input: { index: BCS.U16 },
    Result: { index: BCS.U16 },
    NestedResult: { index: BCS.U16, resultIndex: BCS.U16 },
  })
  .registerStructType(PROGRAMMABLE_CALL_INNER, {
    package: BCS.ADDRESS,
    module: BCS.STRING,
    function: BCS.STRING,
    type_arguments: [VECTOR, TYPE_TAG],
    arguments: [VECTOR, ARGUMENT],
  })
  // Keep this in sync with crates/sui-types/src/messages.rs
  .registerEnumType(TRANSACTION_INNER, {
    /**
     * A Move Call - any public Move function can be called via
     * this transaction. The results can be used that instant to pass
     * into the next transaction.
     */
    MoveCall: PROGRAMMABLE_CALL,
    /**
     * Transfer vector of objects to a receiver.
     */
    TransferObjects: {
      objects: [VECTOR, ARGUMENT],
      address: ARGUMENT,
    },
    /**
     * Split `amount` from a `coin`.
     */
    SplitCoins: { coin: ARGUMENT, amounts: [VECTOR, ARGUMENT] },
    /**
     * Merge Vector of Coins (`sources`) into a `destination`.
     */
    MergeCoins: { destination: ARGUMENT, sources: [VECTOR, ARGUMENT] },
    /**
     * Publish a Move module.
     */
    Publish: {
      modules: [VECTOR, [VECTOR, BCS.U8]],
      dependencies: [VECTOR, BCS.ADDRESS],
    },
    /**
     * Build a vector of objects using the input arguments.
     * It is impossible to construct a `vector<T: key>` otherwise,
     * so this call serves a utility function.
     */
    MakeMoveVec: {
      type: [OPTION, TYPE_TAG],
      objects: [VECTOR, ARGUMENT],
    },
    /**  */
    Upgrade: {
      modules: [VECTOR, [VECTOR, BCS.U8]],
      dependencies: [VECTOR, BCS.ADDRESS],
      packageId: BCS.ADDRESS,
      ticket: ARGUMENT,
    },
  })
  .registerEnumType(COMPRESSED_SIGNATURE, {
    ED25519: ['FixedArray[64]', 'u8'],
    Secp256k1: ['FixedArray[64]', 'u8'],
    Secp256r1: ['FixedArray[64]', 'u8'],
  })
  .registerEnumType(PUBLIC_KEY, {
    ED25519: ['FixedArray[32]', 'u8'],
    Secp256k1: ['FixedArray[33]', 'u8'],
    Secp256r1: ['FixedArray[33]', 'u8'],
  })
  .registerStructType(MULTISIG_PK_MAP, {
    pubKey: PUBLIC_KEY,
    weight: BCS.U8,
  })
  .registerStructType(MULTISIG_PUBLIC_KEY, {
    pk_map: [VECTOR, MULTISIG_PK_MAP],
    threshold: BCS.U16,
  })
  .registerStructType(MULTISIG, {
    sigs: [VECTOR, COMPRESSED_SIGNATURE],
    bitmap: BCS.U16,
    multisig_pk: MULTISIG_PUBLIC_KEY,
  });
/**
 * Utilities for better decoding.
 */

type ProgrammableCallInner = {
  package: string;
  module: string;
  function: string;
  type_arguments: TypeTag[];
  arguments: TransactionArgument[];
};

/**
 * Wrapper around Enum, which transforms any `T` into an object with `kind` property:
 * @example
 * ```
 * let bcsEnum = { TransferObjects: { objects: [], address: ... } }
 * // becomes
 * let translatedEnum = { kind: 'TransferObjects', objects: [], address: ... };
 * ```
 */
builder.registerType(
  [ENUM_KIND, 'T'],
  function encode(
    this: BCS,
    writer,
    data: { kind: string },
    typeParams,
    typeMap,
  ) {
    const kind = data.kind;
    const invariant = { [kind]: data };
    const [enumType] = typeParams;

    return this.getTypeInterface(enumType as string)._encodeRaw.call(
      this,
      writer,
      invariant,
      typeParams,
      typeMap,
    );
  },
  function decode(this: BCS, reader, typeParams, typeMap) {
    const [enumType] = typeParams;
    const data = this.getTypeInterface(enumType as string)._decodeRaw.call(
      this,
      reader,
      typeParams,
      typeMap,
    );

    // enum invariant can only have one `key` field
    const kind = Object.keys(data)[0];
    return { kind, ...data[kind] };
  },
  (data: { kind: string }) => {
    if (typeof data !== 'object' && !('kind' in data)) {
      throw new Error(
        `EnumKind: Missing property "kind" in the input ${JSON.stringify(
          data,
        )}`,
      );
    }

    return true;
  },
);

/**
 * Custom deserializer for the ProgrammableCall.
 *
 * Hides the inner structure and gives a simpler, more convenient
 * interface to encode and decode this struct as a part of `TransactionData`.
 *
 * - `(package)::(module)::(function)` are now `target` property.
 * - `TypeTag[]` array is now passed as strings, not as a struct.
 */
builder.registerType(
  PROGRAMMABLE_CALL,
  function encodeProgrammableTx(
    this: BCS,
    writer,
    data: MoveCallTransaction,
    typeParams,
    typeMap,
  ) {
    const [pkg, module, fun] = data.target.split('::');
    const type_arguments = data.typeArguments.map((tag) =>
      TypeTagSerializer.parseFromStr(tag, true),
    );

    return this.getTypeInterface(PROGRAMMABLE_CALL_INNER)._encodeRaw.call(
      this,
      writer,
      {
        package: normalizeSuiAddress(pkg),
        module,
        function: fun,
        type_arguments,
        arguments: data.arguments,
      } as ProgrammableCallInner,
      typeParams,
      typeMap,
    );
  },
  function decodeProgrammableTx(this: BCS, reader, typeParams, typeMap) {
    let data: ProgrammableCallInner = builder
      .getTypeInterface(PROGRAMMABLE_CALL_INNER)
      ._decodeRaw.call(this, reader, typeParams, typeMap);

    return {
      target: [data.package, data.module, data.function].join('::'),
      arguments: data.arguments,
      typeArguments: data.type_arguments.map(TypeTagSerializer.tagToString),
    };
  },
  // Validation callback to error out if the data format is invalid.
  // TODO: make sure TypeTag can be parsed.
  (data: MoveCallTransaction) => {
    return data.target.split('::').length === 3;
  },
);

function registerFixedArray(bcs: BCS, name: string, length: number) {
  bcs.registerType(
    name,
    function encode(this: BCS, writer, data, typeParams, typeMap) {
      if (data.length !== length) {
        throw new Error(
          `Expected fixed array of length ${length}, got ${data.length}`,
        );
      }

      if (typeParams.length !== 1) {
        throw new Error(
          `Expected one type parameter in a fixed array, got ${typeParams.length}`,
        );
      }

      let [type] =
        typeof typeParams[0] === 'string' ? [typeParams[0], []] : typeParams[0];

      for (let piece of data) {
        this.getTypeInterface(type)._encodeRaw.call(
          this,
          writer,
          piece,
          typeParams,
          typeMap,
        );
      }

      return writer;
    },
    function decode(this: BCS, reader, typeParams, typeMap) {
      if (typeParams.length !== 1) {
        throw new Error(
          `Expected one type parameter in a fixed array, got ${typeParams.length}`,
        );
      }

      let result: any = [];
      let [type] =
        typeof typeParams[0] === 'string' ? [typeParams[0], []] : typeParams[0];

      for (let i = 0; i < length; i++) {
        result.push(
          this.getTypeInterface(type)._decodeRaw.call(
            this,
            reader,
            typeParams,
            typeMap,
          ),
        );
      }

      return result;
    },
  );
}
