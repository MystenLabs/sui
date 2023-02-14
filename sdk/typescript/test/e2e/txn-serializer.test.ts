// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  bcsForVersion,
  deserializeTransactionBytesToTransactionData,
  LocalTxnDataSerializer,
  MoveCallTransaction,
  PureArg,
  RawSigner,
  RpcTxnDataSerializer,
  SuiMoveObject,
  SUI_SYSTEM_STATE_OBJECT_ID,
  UnserializedSignableTransaction,
} from '../../src';
import { CallArgSerializer } from '../../src/signers/txn-data-serializers/call-arg-serializer';
import {
  DEFAULT_GAS_BUDGET,
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
      toolbox.provider.endpoints.fullNode,
    );
    const signer = new RawSigner(toolbox.keypair, toolbox.provider);
    const packagePath = __dirname + '/./data/serializer';
    packageId = await publishPackage(signer, false, packagePath);
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
      packageObjectId: '0000000000000000000000000000000000000002',
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

    const validators = await toolbox.getActiveValidators();
    const validator_metadata = (validators[0] as SuiMoveObject).fields.metadata;
    const validator_address = (validator_metadata as SuiMoveObject).fields
      .sui_address;

    const moveCall = {
      packageObjectId: '0000000000000000000000000000000000000002',
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
    const serArgsExpected = await new CallArgSerializer(
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
    const serArgs = await new CallArgSerializer(
      toolbox.provider,
    ).serializeMoveCallArguments(moveCall);
    expect(serArgs).toEqual(serArgsExpected);
  });
});
