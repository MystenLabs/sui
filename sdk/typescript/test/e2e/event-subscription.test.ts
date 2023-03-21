// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, vi } from 'vitest';
import { SuiEvent, Transaction } from '../../src';
import { setup, TestToolbox } from './utils/setup';

describe('Event Subscription API', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  const mockCallback = vi.fn((_: SuiEvent) => expect(true).toBeTruthy());

  it('Subscribe to events', async () => {
    const subscriptionId = await toolbox.provider.subscribeEvent({
      filter: { Sender: toolbox.address() },
      onMessage: mockCallback,
    });

    const tx = new Transaction();
    tx.moveCall({
      target: '0x2::devnet_nft::mint',
      arguments: [
        tx.pure('Example NFT'),
        tx.pure('An NFT created by the wallet Command Line Tool'),
        tx.pure(
          'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
        ),
      ],
    });
    await toolbox.signer.signAndExecuteTransaction({
      transaction: tx,
      requestType: 'WaitForLocalExecution',
    });

    const subFoundAndRemoved = await toolbox.provider.unsubscribeEvent({
      id: subscriptionId,
    });
    expect(subFoundAndRemoved).toBeTruthy();
    expect(mockCallback).toHaveBeenCalled();
  });
});
