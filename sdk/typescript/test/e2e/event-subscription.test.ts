// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, vi } from 'vitest';
import { RawSigner, SuiEventEnvelope } from '../../src';
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

    const inputCoins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );

    await signer.signAndExecuteTransaction({
      kind: 'payAllSui',
      data: {
        inputCoins: inputCoins.map((o) => o.objectId),
        recipient: DEFAULT_RECIPIENT,
        gasBudget: DEFAULT_GAS_BUDGET,
      },
    });

    const subFoundAndRemoved = await toolbox.provider.unsubscribeEvent(
      subscriptionId,
    );
    expect(subFoundAndRemoved).toBeTruthy();
    expect(mockCallback).toHaveBeenCalled();
  });
});
