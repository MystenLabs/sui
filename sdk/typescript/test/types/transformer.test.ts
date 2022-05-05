// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockObjectData from '../mocks/data/objects.json';
import { transformObjectContent } from '../../src/types/framework/transformer';

import {
  getObjectContent,
  GetObjectInfoResponse,
  ObjectContent,
} from '../../src';
import { isGetObjectInfoResponse } from '../../src/index.guard';

describe('Test simplify common Move structs', () => {
  it('Test with Coin', () => {
    const coin = getContent('coin');
    const expected = {
      ...coin,
      fields: {
        ...coin.fields,
        balance: 50000,
      },
    };
    replaceField(expected, 'id', '07db46736b11cc9e46ea2bbcaf4b71bea706ea4e');
    expect(transformObjectContent(coin)).toEqual(expected);
  });

  it('Test with Example NFT', () => {
    const example_nft = getContent('example_nft');
    const expected = {
      ...example_nft,
      fields: {
        ...example_nft.fields,
        description: 'An NFT created by the wallet Command Line Tool',
        name: 'Example NFT',
      },
    } as ObjectContent;
    replaceField(expected, 'id', 'be64c8f6d5fe15799c46f245d1211f1b084e589c');
    replaceField(
      expected,
      'url',
      'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty'
    );
    expect(transformObjectContent(example_nft)).toEqual(expected);
  });
});

function replaceField(data: ObjectContent, key: string, id: string) {
  (data.fields[key] as ObjectContent)['fields'][key] = id;
}

function getContent(key: 'coin' | 'example_nft'): ObjectContent {
  return getObjectContent(validate(mockObjectData[key]))!;
}

function validate(data: any): GetObjectInfoResponse {
  expect(isGetObjectInfoResponse(data)).toBeTruthy();
  return data;
}
