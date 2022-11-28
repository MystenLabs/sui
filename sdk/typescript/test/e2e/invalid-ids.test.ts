// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { setup, TestToolbox } from './utils/setup';


describe('Not empty object validation', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

//Test that with invalid object id/address/digest, functions will throw an error before making a request to the rpc server
  it('Test all functions with invalid Sui Address', async () => {
    expect(toolbox.provider.getObjectsOwnedByAddress('0xree86ca3d95b95c0f8ecbe06c71d925b3b75470b')).rejects.toThrowError(/Invalid Sui address/);
    expect(toolbox.provider.getTransactionsForAddress('QQQ')).rejects.toThrowError(/Invalid Sui address/);
  })

  it('Test all functions with invalid Object Id', async () => {
    expect(toolbox.provider.getObject('')).rejects.toThrowError(/Invalid Sui Object id/);
    expect(toolbox.provider.getObjectsOwnedByObject('0x4ce52ee7b659b610d59a1ced129291b3d0d421632')).rejects.toThrowError(/Invalid Sui Object id/);
    expect(toolbox.provider.getTransactionsForObject('4ce52ee7b659b610d59a1ced129291b3d0d421632')).rejects.toThrowError(/Invalid Sui Object id/);
  })

  it('Test all functions with invalid Transaction Digest', async () => {
    expect(toolbox.provider.getTransactionWithEffects('')).rejects.toThrowError(/Invalid Transaction digest/);
  })
});