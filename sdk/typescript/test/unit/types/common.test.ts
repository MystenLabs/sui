// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { bcs, generateTransactionDigest } from '../../../src';

describe('Test common functions', () => {
  describe('Calculate transaction digest', () => {
    it('Test calculate transaction digest for ED25519', () => {
      const transactionData = {
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
          payment: {
            objectId: '2fab642a835afc9d68d296f50c332c9d32b5a0d5',
            version: 7,
            digest: 'lGmQDt2ch1/4HwdgOlHmeeZZvCHUjfrKvBOND/c67n4=',
          },
          price: 1,
          budget: 100,
        },
      };

      const transactionDigest = generateTransactionDigest(transactionData, bcs);
      expect(transactionDigest).toEqual(
        '3DBBCLZWejuZWVbGPHB3n4AtWjNf4gHWDUPV1hE45Kb9',
      );
    });
  });
});
