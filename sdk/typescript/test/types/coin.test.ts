// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '../../src';

const COIN_DATA = {
  fields: {
    id: {
      fields: {
        id: {
          fields: {
            id: {
              fields: {
                bytes: '37196de8502e6d80e6a31fba1a5d6986cc018805',
              },
              type: '0x2::ID::ID',
            },
          },
          type: '0x2::ID::UniqueID',
        },
        version: 0,
      },
      type: '0x2::ID::VersionedID',
    },
    value: 100000,
  },
  type: '0x2::Coin::Coin<0x2::SUI::SUI>',
};

describe('Coin type Parsing', () => {
  it('parse Coin Type', async () => {
    const t = new Coin(COIN_DATA);
    expect(t.symbol()).toEqual('SUI');
    expect(t.amount()).toEqual(100000);
    expect(t.id().id()).toEqual('37196de8502e6d80e6a31fba1a5d6986cc018805');
    expect(t.toJSON()).toEqual(
      '{"amount":100000,"symbol":"SUI","versioned_id":"{\\"id\\":\\"37196de8502e6d80e6a31fba1a5d6986cc018805\\",\\"version\\":0}"}'
    );
  });
});
