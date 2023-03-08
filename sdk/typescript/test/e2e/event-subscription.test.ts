// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, vi } from 'vitest';
import { Commands, RawSigner, SuiEventEnvelope, Transaction } from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  DEFAULT_RECIPIENT,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Event Subscription API', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
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
    tx.add(Commands.TransferObjects([tx.gas], tx.input(DEFAULT_RECIPIENT)));
    await signer.signAndExecuteTransaction(tx);

    const subFoundAndRemoved = await toolbox.provider.unsubscribeEvent(
      subscriptionId,
    );
    expect(subFoundAndRemoved).toBeTruthy();
    expect(mockCallback).toHaveBeenCalled();
  });
});
