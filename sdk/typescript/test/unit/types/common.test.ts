// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { bcs, generateTransactionDigest, TransactionData } from '../../../src';

describe('Test common functions', () => {
  describe('Calculate transaction digest', () => {
    it('Test calculate transaction digest for ED25519', () => {
      const transactionData: TransactionData = {
        messageVersion: 1,
        kind: {
          Single: {
            TransferSui: {
              recipient: 'cba4a48bb0f8b586c167e5dcefaa1c5e96ab3f08',
              amount: {
                Some: 1,
              },
            },
          },
        },
        sender: 'cba4a48bb0f8b586c167e5dcefaa1c5e96ab3f08',
        gasData: {
          owner: 'cba4a48bb0f8b586c167e5dcefaa1c5e96ab3f08',
          payment: [
            {
              objectId: '2fab642a835afc9d68d296f50c332c9d32b5a0d5',
              version: 7,
              digest: 'DjnxhsPchJGa5crALRp8coJazNvV4s3mqpdcxVVKJrpt',
            },
          ],
          price: 1,
          budget: 100,
        },
        expiration: { None: null },
      };

      const transactionDigest = generateTransactionDigest(transactionData, bcs);
      expect(transactionDigest).toEqual(
        'EgZ8XyQ1BKgWsa7r8baN5HPRoKcvm5ZskJUPxdqBjCeN',
      );
    });
  });
});
