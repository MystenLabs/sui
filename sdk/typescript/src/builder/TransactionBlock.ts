// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { is, mask } from 'superstruct';
import { JsonRpcProvider } from '../providers/json-rpc-provider';
import {
  extractMutableReference,
  extractStructTag,
  getObjectReference,
  getSharedObjectInitialVersion,
  normalizeSuiObjectId,
  ObjectId,
  SuiMoveNormalizedType,
  SuiObjectRef,
  SUI_TYPE_ARG,
} from '../types';
import {
  Transactions,
  TransactionArgument,
  TransactionType,
  TransactionBlockInput,
  getTransactionType,
  MoveCallTransaction,
} from './Transactions';
import {
  BuilderCallArg,
  getIdFromCallArg,
  Inputs,
  isMutableSharedObjectInput,
  ObjectCallArg,
} from './Inputs';
import { getPureSerializationType, isTxContext } from './serializer';
import {
  TransactionBlockDataBuilder,
  TransactionExpiration,
} from './TransactionBlockData';
import { TRANSACTION_TYPE, create, WellKnownEncoding } from './utils';

type TransactionResult = TransactionArgument & TransactionArgument[];

function createTransactionResult(index: number): TransactionResult {
  const baseResult: TransactionArgument = { kind: 'Result', index };

  const nestedResults: TransactionArgument[] = [];
  const nestedResultFor = (resultIndex: number): TransactionArgument =>
    (nestedResults[resultIndex] ??= {
      kind: 'NestedResult',
      index,
      resultIndex,
    });

  return new Proxy(baseResult, {
    set() {
      throw new Error(
        'The transaction result is a proxy, and does not support setting properties directly',
      );
    },
    // TODO: Instead of making this return a concrete argument, we should ideally
    // make it reference-based (so that this gets resolved at build-time), which
    // allows re-ordering transactions.
    get(target, property) {
      // This allows this transaction argument to be used in the singular form:
      if (property in target) {
        return Reflect.get(target, property);
      }

      // Support destructuring:
      if (property === Symbol.iterator) {
        return function* () {
          let i = 0;
          while (true) {
            yield nestedResultFor(i);
            i++;
          }
        };
      }

      if (typeof property === 'symbol') return;

      const resultIndex = parseInt(property, 10);
      if (Number.isNaN(resultIndex) || resultIndex < 0) return;
      return nestedResultFor(resultIndex);
    },
  }) as TransactionResult;
}

function expectProvider(
  provider: JsonRpcProvider | undefined,
): JsonRpcProvider {
  if (!provider) {
    throw new Error(
      `No provider passed to Transaction#build, but transaction data was not sufficient to build offline.`,
    );
  }

  return provider;
}

const TRANSACTION_BRAND = Symbol.for('@mysten/transaction');

// The maximum number of gas objects that can be selected for one transaction.
const MAX_GAS_OBJECTS = 256;

// The maximum gas that is allowed.
const MAX_GAS = 50_000_000_000;

// An amount of gas (in gas units) that is added to transactions as an overhead to ensure transactions do not fail.
const GAS_SAFE_OVERHEAD = 1000n;

interface BuildOptions {
  provider?: JsonRpcProvider;
  onlyTransactionKind?: boolean;
}

/**
 * Transaction Builder
 */
export class TransactionBlock {
  /** Returns `true` if the object is an instance of the Transaction builder class. */
  static is(obj: unknown): obj is TransactionBlock {
    return (
      !!obj &&
      typeof obj === 'object' &&
      (obj as any)[TRANSACTION_BRAND] === true
    );
  }

  /**
   * Converts from a serialize transaction kind (built with `build({ onlyTransactionKind: true })`) to a `Transaction` class.
   * Supports either a byte array, or base64-encoded bytes.
   */
  static fromKind(serialized: string | Uint8Array) {
    const tx = new TransactionBlock();

    tx.#blockData = TransactionBlockDataBuilder.fromKindBytes(
      typeof serialized === 'string' ? fromB64(serialized) : serialized,
    );

    return tx;
  }

  /**
   * Converts from a serialized transaction format to a `Transaction` class.
   * There are two supported serialized formats:
   * - A string returned from `Transaction#serialize`. The serialized format must be compatible, or it will throw an error.
   * - A byte array (or base64-encoded bytes) containing BCS transaction data.
   */
  static from(serialized: string | Uint8Array) {
    const tx = new TransactionBlock();

    // Check for bytes:
    if (typeof serialized !== 'string' || !serialized.startsWith('{')) {
      tx.#blockData = TransactionBlockDataBuilder.fromBytes(
        typeof serialized === 'string' ? fromB64(serialized) : serialized,
      );
    } else {
      tx.#blockData = TransactionBlockDataBuilder.restore(
        JSON.parse(serialized),
      );
    }

    return tx;
  }

  /** A helper to retrieve the Transaction builder `Transactions` */
  static get Transactions() {
    return Transactions;
  }

  /** A helper to retrieve the Transaction builder `Inputs` */
  static get Inputs() {
    return Inputs;
  }

  setSender(sender: string) {
    this.#blockData.sender = sender;
  }
  /**
   * Sets the sender only if it has not already been set.
   * This is useful for sponsored transaction flows where the sender may not be the same as the signer address.
   */
  setSenderIfNotSet(sender: string) {
    if (!this.#blockData.sender) {
      this.#blockData.sender = sender;
    }
  }
  setExpiration(expiration?: TransactionExpiration) {
    this.#blockData.expiration = expiration;
  }
  setGasPrice(price: number | bigint) {
    this.#blockData.gasConfig.price = String(price);
  }
  setGasBudget(budget: number | bigint) {
    this.#blockData.gasConfig.budget = String(budget);
  }
  setGasOwner(owner: string) {
    this.#blockData.gasConfig.owner = owner;
  }
  setGasPayment(payments: SuiObjectRef[]) {
    if (payments.length >= MAX_GAS_OBJECTS) {
      throw new Error(
        `Payment objects exceed maximum amount ${MAX_GAS_OBJECTS}`,
      );
    }
    this.#blockData.gasConfig.payment = payments.map((payment) =>
      mask(payment, SuiObjectRef),
    );
  }

  #blockData: TransactionBlockDataBuilder;
  /** Get a snapshot of the transaction data, in JSON form: */
  get blockData() {
    return this.#blockData.snapshot();
  }

  // Used to brand transaction classes so that they can be identified, even between multiple copies
  // of the builder.
  get [TRANSACTION_BRAND]() {
    return true;
  }

  constructor(transaction?: TransactionBlock) {
    this.#blockData = new TransactionBlockDataBuilder(
      transaction ? transaction.blockData : undefined,
    );
  }

  /** Returns an argument for the gas coin, to be used in a transaction. */
  get gas(): TransactionArgument {
    return { kind: 'GasCoin' };
  }

  /**
   * Dynamically create a new input, which is separate from the `input`. This is important
   * for generated clients to be able to define unique inputs that are non-overlapping with the
   * defined inputs.
   *
   * For `Uint8Array` type automatically convert the input into a `Pure` CallArg, since this
   * is the format required for custom serialization.
   *
   */
  #input(type: 'object' | 'pure', value?: unknown) {
    const index = this.#blockData.inputs.length;
    const input = create(
      {
        kind: 'Input',
        // bigints can't be serialized to JSON, so just string-convert them here:
        value: typeof value === 'bigint' ? String(value) : value,
        index,
        type,
      },
      TransactionBlockInput,
    );
    this.#blockData.inputs.push(input);
    return input;
  }

  /**
   * Add a new object input to the transaction.
   */
  object(value: ObjectId | ObjectCallArg) {
    const id = getIdFromCallArg(value);
    // deduplicate
    const inserted = this.#blockData.inputs.find(
      (i) => i.type === 'object' && id === getIdFromCallArg(i.value),
    );
    return inserted ?? this.#input('object', value);
  }

  /**
   * Add a new object input to the transaction using the fully-resolved object reference.
   * If you only have an object ID, use `builder.object(id)` instead.
   */
  objectRef(...args: Parameters<(typeof Inputs)['ObjectRef']>) {
    return this.object(Inputs.ObjectRef(...args));
  }

  /**
   * Add a new shared object input to the transaction using the fully-resolved shared object reference.
   * If you only have an object ID, use `builder.object(id)` instead.
   */
  sharedObjectRef(...args: Parameters<(typeof Inputs)['SharedObjectRef']>) {
    return this.object(Inputs.SharedObjectRef(...args));
  }

  /**
   * Add a new non-object input to the transaction.
   */
  pure(
    /**
     * The pure value that will be used as the input value. If this is a Uint8Array, then the value
     * is assumed to be raw bytes, and will be used directly.
     */
    value: unknown,
    /**
     * The BCS type to serialize the value into. If not provided, the type will automatically be determined
     * based on how the input is used.
     */
    type?: string,
  ) {
    // TODO: we can also do some deduplication here
    return this.#input(
      'pure',
      value instanceof Uint8Array
        ? Inputs.Pure(value)
        : type
        ? Inputs.Pure(value, type)
        : value,
    );
  }

  /** Add a transaction to the transaction block. */
  add(transaction: TransactionType) {
    const index = this.#blockData.transactions.push(transaction);
    return createTransactionResult(index - 1);
  }

  // Method shorthands:

  splitCoins(...args: Parameters<(typeof Transactions)['SplitCoins']>) {
    return this.add(Transactions.SplitCoins(...args));
  }
  mergeCoins(...args: Parameters<(typeof Transactions)['MergeCoins']>) {
    return this.add(Transactions.MergeCoins(...args));
  }
  publish(...args: Parameters<(typeof Transactions)['Publish']>) {
    return this.add(Transactions.Publish(...args));
  }
  upgrade(...args: Parameters<(typeof Transactions)['Upgrade']>) {
    return this.add(Transactions.Upgrade(...args));
  }
  moveCall(...args: Parameters<(typeof Transactions)['MoveCall']>) {
    return this.add(Transactions.MoveCall(...args));
  }
  transferObjects(
    ...args: Parameters<(typeof Transactions)['TransferObjects']>
  ) {
    return this.add(Transactions.TransferObjects(...args));
  }
  makeMoveVec(...args: Parameters<(typeof Transactions)['MakeMoveVec']>) {
    return this.add(Transactions.MakeMoveVec(...args));
  }

  /**
   * Serialize the transaction to a string so that it can be sent to a separate context.
   * This is different from `build` in that it does not serialize to BCS bytes, and instead
   * uses a separate format that is unique to the transaction builder. This allows
   * us to serialize partially-complete transactions, that can then be completed and
   * built in a separate context.
   *
   * For example, a dapp can construct a transaction, but not provide gas objects
   * or a gas budget. The transaction then can be sent to the wallet, where this
   * information is automatically filled in (e.g. by querying for coin objects
   * and performing a dry run).
   */
  serialize() {
    return JSON.stringify(this.#blockData.snapshot());
  }

  /** Build the transaction to BCS bytes. */
  async build({
    provider,
    onlyTransactionKind,
  }: BuildOptions = {}): Promise<Uint8Array> {
    await this.#prepare({ provider, onlyTransactionKind });
    return this.#blockData.build({ onlyTransactionKind });
  }

  /** Derive transaction digest */
  async getDigest({
    provider,
  }: {
    provider?: JsonRpcProvider;
  } = {}): Promise<string> {
    await this.#prepare({ provider });
    return this.#blockData.getDigest();
  }

  // The current default is just picking _all_ coins we can which may not be ideal.
  async #prepareGasPayment({ provider, onlyTransactionKind }: BuildOptions) {
    // Early return if the payment is already set:
    if (onlyTransactionKind || this.#blockData.gasConfig.payment) {
      return;
    }

    const gasOwner = this.#blockData.gasConfig.owner ?? this.#blockData.sender;

    const coins = await expectProvider(provider).getCoins({
      owner: gasOwner!,
      coinType: SUI_TYPE_ARG,
    });

    const paymentCoins = coins.data
      // Filter out coins that are also used as input:
      .filter((coin) => {
        const matchingInput = this.#blockData.inputs.find((input) => {
          if (
            is(input.value, BuilderCallArg) &&
            'Object' in input.value &&
            'ImmOrOwned' in input.value.Object
          ) {
            return coin.coinObjectId === input.value.Object.ImmOrOwned.objectId;
          }

          return false;
        });

        return !matchingInput;
      })
      .slice(0, MAX_GAS_OBJECTS - 1)
      .map((coin) => ({
        objectId: coin.coinObjectId,
        digest: coin.digest,
        version: coin.version,
      }));

    if (!paymentCoins.length) {
      throw new Error('No valid gas coins found for the transaction.');
    }

    this.setGasPayment(paymentCoins);
  }

  async #prepareGasPrice({ provider, onlyTransactionKind }: BuildOptions) {
    if (onlyTransactionKind || this.#blockData.gasConfig.price) {
      return;
    }

    this.setGasPrice(await expectProvider(provider).getReferenceGasPrice());
  }

  async #prepareTransactions(provider?: JsonRpcProvider) {
    const { inputs, transactions } = this.#blockData;

    const moveModulesToResolve: MoveCallTransaction[] = [];

    // Keep track of the object references that will need to be resolved at the end of the transaction.
    // We keep the input by-reference to avoid needing to re-resolve it:
    const objectsToResolve: {
      id: string;
      input: TransactionBlockInput;
      normalizedType?: SuiMoveNormalizedType;
    }[] = [];

    transactions.forEach((transaction) => {
      // Special case move call:
      if (transaction.kind === 'MoveCall') {
        // Determine if any of the arguments require encoding.
        // - If they don't, then this is good to go.
        // - If they do, then we need to fetch the normalized move module.
        const needsResolution = transaction.arguments.some(
          (arg) =>
            arg.kind === 'Input' &&
            !is(inputs[arg.index].value, BuilderCallArg),
        );

        if (needsResolution) {
          moveModulesToResolve.push(transaction);
        }

        return;
      }

      // Get the matching struct definition for the transaction, and use it to attempt to automatically
      // encode the matching inputs.
      const transactionType = getTransactionType(transaction);
      if (!transactionType.schema) return;

      Object.entries(transaction).forEach(([key, value]) => {
        if (key === 'kind') return;
        const keySchema = (transactionType.schema as any)[key];
        const isArray = keySchema.type === 'array';
        const wellKnownEncoding: WellKnownEncoding = isArray
          ? keySchema.schema[TRANSACTION_TYPE]
          : keySchema[TRANSACTION_TYPE];

        // This argument has unknown encoding, assume it must be fully-encoded:
        if (!wellKnownEncoding) return;

        const encodeInput = (index: number) => {
          const input = inputs[index];
          if (!input) {
            throw new Error(`Missing input ${value.index}`);
          }

          // Input is fully resolved:
          if (is(input.value, BuilderCallArg)) return;
          if (
            wellKnownEncoding.kind === 'object' &&
            typeof input.value === 'string'
          ) {
            // The input is a string that we need to resolve to an object reference:
            objectsToResolve.push({ id: input.value, input });
          } else if (wellKnownEncoding.kind === 'pure') {
            // Pure encoding, so construct BCS bytes:
            input.value = Inputs.Pure(input.value, wellKnownEncoding.type);
          } else {
            throw new Error('Unexpected input format.');
          }
        };

        if (isArray) {
          value.forEach((arrayItem: TransactionArgument) => {
            if (arrayItem.kind !== 'Input') return;
            encodeInput(arrayItem.index);
          });
        } else {
          if (value.kind !== 'Input') return;
          encodeInput(value.index);
        }
      });
    });

    if (moveModulesToResolve.length) {
      await Promise.all(
        moveModulesToResolve.map(async (moveCall) => {
          const [packageId, moduleName, functionName] =
            moveCall.target.split('::');

          const normalized = await expectProvider(
            provider,
          ).getNormalizedMoveFunction({
            package: normalizeSuiObjectId(packageId),
            module: moduleName,
            function: functionName,
          });

          // Entry functions can have a mutable reference to an instance of the TxContext
          // struct defined in the TxContext module as the last parameter. The caller of
          // the function does not need to pass it in as an argument.
          const hasTxContext =
            normalized.parameters.length > 0 &&
            isTxContext(normalized.parameters.at(-1)!);

          const params = hasTxContext
            ? normalized.parameters.slice(0, normalized.parameters.length - 1)
            : normalized.parameters;

          if (params.length !== moveCall.arguments.length) {
            throw new Error('Incorrect number of arguments.');
          }

          params.forEach((param, i) => {
            const arg = moveCall.arguments[i];
            if (arg.kind !== 'Input') return;
            const input = inputs[arg.index];
            // Skip if the input is already resolved
            if (is(input.value, BuilderCallArg)) return;

            const inputValue = input.value;

            const serType = getPureSerializationType(param, inputValue);

            if (serType) {
              input.value = Inputs.Pure(inputValue, serType);
              return;
            }

            const structVal = extractStructTag(param);
            if (
              structVal != null ||
              (typeof param === 'object' && 'TypeParameter' in param)
            ) {
              if (typeof inputValue !== 'string') {
                throw new Error(
                  `Expect the argument to be an object id string, got ${JSON.stringify(
                    inputValue,
                    null,
                    2,
                  )}`,
                );
              }
              objectsToResolve.push({
                id: inputValue,
                input,
                normalizedType: param,
              });
              return;
            }

            throw new Error(
              `Unknown call arg type ${JSON.stringify(
                param,
                null,
                2,
              )} for value ${JSON.stringify(inputValue, null, 2)}`,
            );
          });
        }),
      );
    }

    if (objectsToResolve.length) {
      const dedupedIds = [...new Set(objectsToResolve.map(({ id }) => id))];
      const objects = await expectProvider(provider).multiGetObjects({
        ids: dedupedIds,
        options: { showOwner: true },
      });
      let objectsById = new Map(
        dedupedIds.map((id, index) => {
          return [id, objects[index]];
        }),
      );

      const invalidObjects = Array.from(objectsById)
        .filter(([_, obj]) => obj.error)
        .map(([id, _]) => id);
      if (invalidObjects.length) {
        throw new Error(
          `The following input objects are not invalid: ${invalidObjects.join(
            ', ',
          )}`,
        );
      }

      objectsToResolve.forEach(({ id, input, normalizedType }) => {
        const object = objectsById.get(id)!;
        const initialSharedVersion = getSharedObjectInitialVersion(object);

        if (initialSharedVersion) {
          // There could be multiple transactions that reference the same shared object.
          // If one of them is a mutable reference, then we should mark the input
          // as mutable.
          const mutable =
            isMutableSharedObjectInput(input.value) ||
            (normalizedType != null &&
              extractMutableReference(normalizedType) != null);

          input.value = Inputs.SharedObjectRef({
            objectId: id,
            initialSharedVersion,
            mutable,
          });
        } else {
          input.value = Inputs.ObjectRef(getObjectReference(object)!);
        }
      });
    }
  }

  /**
   * Prepare the transaction by valdiating the transaction data and resolving all inputs
   * so that it can be built into bytes.
   */
  async #prepare({ provider, onlyTransactionKind }: BuildOptions) {
    if (!onlyTransactionKind && !this.#blockData.sender) {
      throw new Error('Missing transaction sender');
    }

    await Promise.all([
      this.#prepareGasPrice({ provider, onlyTransactionKind }),
      this.#prepareTransactions(provider),
    ]);

    if (!onlyTransactionKind) {
      await this.#prepareGasPayment({ provider, onlyTransactionKind });

      if (!this.#blockData.gasConfig.budget) {
        const dryRunResult = await expectProvider(
          provider,
        ).dryRunTransactionBlock({
          transactionBlock: this.#blockData.build({
            overrides: {
              gasConfig: {
                budget: String(MAX_GAS),
                payment: [],
              },
            },
          }),
        });
        if (dryRunResult.effects.status.status !== 'success') {
          throw new Error(
            `Dry run failed, could not automatically determine a budget: ${dryRunResult.effects.status.error}`,
            { cause: dryRunResult },
          );
        }

        const safeOverhead =
          GAS_SAFE_OVERHEAD * BigInt(this.blockData.gasConfig.price || 1n);

        const baseComputationCostWithOverhead =
          BigInt(dryRunResult.effects.gasUsed.computationCost) + safeOverhead;

        const gasBudget =
          baseComputationCostWithOverhead +
          BigInt(dryRunResult.effects.gasUsed.storageCost) -
          BigInt(dryRunResult.effects.gasUsed.storageRebate);

        // Set the budget to max(computation, computation + storage - rebate)
        this.setGasBudget(
          gasBudget > baseComputationCostWithOverhead
            ? gasBudget
            : baseComputationCostWithOverhead,
        );
      }
    }
  }
}
