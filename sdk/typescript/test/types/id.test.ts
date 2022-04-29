// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MoveVersionedID } from '../../src';

const DATA = {
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
};

describe('ID Types Parsing', () => {
  it('parse ID Type', async () => {
    const t = new MoveVersionedID(DATA);
    expect(t.id()).toEqual('37196de8502e6d80e6a31fba1a5d6986cc018805');
    expect(t.version()).toEqual(0);
    expect(t.toJSON()).toEqual(
      '{"id":"37196de8502e6d80e6a31fba1a5d6986cc018805","version":0}'
    );
  });
});
