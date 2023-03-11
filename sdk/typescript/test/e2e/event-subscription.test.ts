// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, vi } from 'vitest';
import { SuiEventEnvelope, Transaction } from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  DEFAULT_RECIPIENT,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Event Subscription API', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  const mockCallback = vi.fn((_: SuiEventEnvelope) =>
    expect(true).toBeTruthy(),
  );

  it('Subscribe to events', async () => {
    const subscriptionId = await toolbox.provider.subscribeEvent(
      { SenderAddress: toolbox.address() },
      mockCallback,
    );

    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    tx.transferObjects([tx.gas], tx.pure(DEFAULT_RECIPIENT));
    await toolbox.signer.signAndExecuteTransaction(
      tx,
      {},
      'WaitForLocalExecution',
    );

    const subFoundAndRemoved = await toolbox.provider.unsubscribeEvent(
      subscriptionId,
    );
    expect(subFoundAndRemoved).toBeTruthy();
    expect(mockCallback).toHaveBeenCalled();
  });
});
