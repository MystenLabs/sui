// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '@mysten/sui.js';
import { TransferPolicy, bcs } from '../bcs';

/** Name of the event emitted when a TransferPolicy for T is created. */
export const TRANSFER_POLICY_CREATED_EVENT = `0x2::transfer_policy::TransferPolicyCreated`;

/**
 *
 * @param provider
 * @param type
 */
export async function queryTransferPolicy(
  provider: JsonRpcProvider,
  type: string,
): Promise<TransferPolicy[]> {
  // console.log('event type: %s', `${TRANSFER_POLICY_CREATED_EVENT}<${type}>`);
  const { data } = await provider.queryEvents({
    query: {
      MoveEventType: `${TRANSFER_POLICY_CREATED_EVENT}<${type}>`,
    },
  });

  const search = data.map((event) => event.parsedJson as { id: string });
  const policies = await Promise.all(
    search.map(async (policy) => {
      const search = await provider.getObject({
        id: policy.id,
        options: { showBcs: true, showOwner: true },
      });

      if ('err' in data || !('data' in search)) {
        return null;
      }

      return search.data;
    }),
  );

  return policies
    .filter((policy) => policy !== null)
    .map((policy) => {
      let parsed = bcs.de(
        'TransferPolicy',
        // @ts-ignore // this structure is not expected (expects UInt8Array, not number[])
        policy?.bcs.bcsBytes!,
        'base64',
      ) as TransferPolicy;

      return {
        // ...policy, // @ts-ignore // until bcs definition is fixed
        id: policy?.objectId,
        type: `0x2::transfer_policy::TransferPolicy<${type}>`,
        owner: policy?.owner,
        rules: parsed.rules,
        balance: parsed.balance,
      } as TransferPolicy;
    });
}
