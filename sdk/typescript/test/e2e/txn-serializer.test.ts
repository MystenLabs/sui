// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  bcsForVersion,
  deserializeTransactionBytesToTransactionData,
  LocalTxnDataSerializer,
  MoveCallTransaction,
  PaySuiTx,
  PureArg,
  RawSigner,
  RpcTxnDataSerializer,
  SUI_SYSTEM_STATE_OBJECT_ID,
  UnserializedSignableTransaction,
  getObjectReference,
  TransactionData,
  TransactionKind,
  PaySuiTransaction,
  getObjectId,
  PayAllSuiTx,
  PayAllSuiTransaction,
} from '../../src';
import { CallArgSerializer } from '../../src/signers/txn-data-serializers/call-arg-serializer';
import {
  DEFAULT_GAS_BUDGET,
  DEFAULT_RECIPIENT,
  DEFAULT_RECIPIENT_2,
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Transaction Serialization and deserialization', () => {
  let toolbox: TestToolbox;
  let localSerializer: LocalTxnDataSerializer;
  let rpcSerializer: RpcTxnDataSerializer;
  let packageId: string;

  beforeAll(async () => {
    toolbox = await setup();
    localSerializer = new LocalTxnDataSerializer(toolbox.provider);
    rpcSerializer = new RpcTxnDataSerializer(
      toolbox.provider.connection.fullnode,
    );
    const signer = new RawSigner(toolbox.keypair, toolbox.provider);
    const packagePath = __dirname + '/./data/serializer';
    packageId = await publishPackage(signer, packagePath);
  });

  async function serializeAndDeserialize(
    moveCall: MoveCallTransaction,
  ): Promise<MoveCallTransaction> {
    const rpcTxnBytes = await rpcSerializer.serializeToBytes(
      toolbox.address(),
      { kind: 'moveCall', data: moveCall },
    );
    const localTxnBytes = await localSerializer.serializeToBytes(
      toolbox.address(),
      { kind: 'moveCall', data: moveCall },
    );

    expect(rpcTxnBytes).toEqual(localTxnBytes);

    const deserialized =
      (await localSerializer.deserializeTransactionBytesToSignableTransaction(
        localTxnBytes,
      )) as UnserializedSignableTransaction;
    expect(deserialized.kind).toEqual('moveCall');

    const deserializedTxnData = deserializeTransactionBytesToTransactionData(
      bcsForVersion(await toolbox.provider.getRpcApiVersion()),
      localTxnBytes,
    );
    const reserialized = await localSerializer.serializeTransactionData(
      deserializedTxnData,
    );
    expect(reserialized).toEqual(localTxnBytes);
    if ('moveCall' === deserialized.kind) {
      const normalized = {
        ...deserialized.data,
        gasBudget: Number(deserialized.data.gasBudget!.toString(10)),
        gasPayment: '0x' + deserialized.data.gasPayment,
        gasPrice: Number(deserialized.data.gasPrice!.toString(10)),
      };
      return normalized;
    }

    throw new Error('unreachable');
  }

  it('Move Call', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    const moveCall = {
      packageObjectId:
        '0000000000000000000000000000000000000000000000000000000000000002',
      module: 'devnet_nft',
      function: 'mint',
      typeArguments: [],
      arguments: [
        'Example NFT',
        'An NFT created by the wallet Command Line Tool',
        'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
      ],
      gasOwner: toolbox.address(),
      gasBudget: DEFAULT_GAS_BUDGET,
      gasPayment: coins[0].objectId,
    };

    const deserialized = await serializeAndDeserialize(moveCall);
    expect(deserialized).toEqual(moveCall);
  });

  it('Move Call With Type Tags', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    const moveCall = {
      packageObjectId: packageId,
      module: 'serializer_tests',
      function: 'list',
      typeArguments: ['0x2::coin::Coin<0x2::sui::SUI>', '0x2::sui::SUI'],
      arguments: [coins[0].objectId],
      gasBudget: DEFAULT_GAS_BUDGET,
    };
    await serializeAndDeserialize(moveCall);
  });

  it('Move Shared Object Call', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );

    const [{ sui_address: validator_address }] =
      await toolbox.getActiveValidators();

    const moveCall = {
      packageObjectId:
        '0000000000000000000000000000000000000000000000000000000000000002',
      module: 'sui_system',
      function: 'request_add_delegation',
      typeArguments: [],
      arguments: [
        SUI_SYSTEM_STATE_OBJECT_ID,
        coins[2].objectId,
        validator_address,
      ],
      gasOwner: toolbox.address(),
      gasBudget: DEFAULT_GAS_BUDGET,
      gasPayment: coins[3].objectId,
    };

    const deserialized = await serializeAndDeserialize(moveCall);
    const normalized = {
      ...deserialized,
      arguments: deserialized.arguments.map((d) => '0x' + d),
    };
    expect(normalized).toEqual(moveCall);
  });

  it('Move Call with Pure Arg', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    const moveCallExpected = {
      packageObjectId: '0x2',
      module: 'devnet_nft',
      function: 'mint',
      typeArguments: [],
      arguments: [
        'Example NFT',
        'An NFT created by the wallet Command Line Tool',
        'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
      ],
      gasBudget: DEFAULT_GAS_BUDGET,
      gasPayment: coins[0].objectId,
    } as MoveCallTransaction;
    const setArgsExpected = await new CallArgSerializer(
      toolbox.provider,
    ).serializeMoveCallArguments(moveCallExpected);

    const version = await toolbox.provider.getRpcApiVersion();
    const pureArg: PureArg = {
      Pure: bcsForVersion(version).ser('string', 'Example NFT').toBytes(),
    };
    const moveCall = {
      packageObjectId: '0x2',
      module: 'devnet_nft',
      function: 'mint',
      typeArguments: [],
      arguments: [
        pureArg,
        'An NFT created by the wallet Command Line Tool',
        'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
      ],
      gasBudget: DEFAULT_GAS_BUDGET,
      gasPayment: coins[0].objectId,
    } as MoveCallTransaction;
    const setArgs = await new CallArgSerializer(
      toolbox.provider,
    ).serializeMoveCallArguments(moveCall);
    expect(setArgs).toEqual(setArgsExpected);
  });

  it('Serialize and deserialize paySui', async () => {
    const gasBudget = 1000;
    const coins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(DEFAULT_GAS_BUDGET),
      );

    const paySuiTx = {
      PaySui: {
        coins: [getObjectReference(coins[0])],
        recipients: [DEFAULT_RECIPIENT],
        amounts: [100],
      },
    } as PaySuiTx;

    const tx_data = {
      messageVersion: 1,
      sender: DEFAULT_RECIPIENT_2,
      kind: { Single: paySuiTx } as TransactionKind,
      gasData: {
        owner: DEFAULT_RECIPIENT_2,
        budget: gasBudget,
        price: 100,
        payment: [getObjectReference(coins[1])],
      },
      expiration: { None: null },
    } as TransactionData;

    const serializedData = await localSerializer.serializeTransactionData(
      tx_data,
    );

    const deserialized =
      await localSerializer.deserializeTransactionBytesToSignableTransaction(
        serializedData,
      );

    const expectedTx = {
      kind: 'paySui',
      data: {
        inputCoins: [getObjectId(coins[0]).substring(2)],
        recipients: [DEFAULT_RECIPIENT.substring(2)],
        amounts: [BigInt(100)] as unknown as number[],
      } as PaySuiTransaction,
    } as UnserializedSignableTransaction;
    expect(expectedTx).toEqual(deserialized);
  });

  it('Serialize and deserialize payAllSui', async () => {
    const gasBudget = 1000;
    const coins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(DEFAULT_GAS_BUDGET),
      );

    const payAllSui = {
      PayAllSui: {
        coins: [getObjectReference(coins[0])],
        recipient: DEFAULT_RECIPIENT,
      },
    } as PayAllSuiTx;
    const tx_data = {
      messageVersion: 1,
      sender: DEFAULT_RECIPIENT_2,
      kind: { Single: payAllSui } as TransactionKind,
      gasData: {
        owner: DEFAULT_RECIPIENT_2,
        budget: gasBudget,
        price: 100,
        payment: [getObjectReference(coins[1])],
      },
      expiration: { None: null },
    } as TransactionData;

    const serializedData = await localSerializer.serializeTransactionData(
      tx_data,
    );

    const deserialized =
      await localSerializer.deserializeTransactionBytesToSignableTransaction(
        serializedData,
      );

    const expectedTx = {
      kind: 'payAllSui',
      data: {
        inputCoins: [getObjectId(coins[0]).substring(2)],
        recipient: DEFAULT_RECIPIENT.substring(2),
      } as PayAllSuiTransaction,
    } as UnserializedSignableTransaction;
    expect(expectedTx).toEqual(deserialized);
  });
});
