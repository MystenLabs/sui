// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockObjectData from '../mocks/data/objects.json';

import { isGetObjectInfoResponse } from '../../src/index.guard';
import { GetObjectInfoResponse } from '../../src';

describe('Test Objects Definition', () => {
  it('Test against different object definitions', () => {
    validate('coin');
    validate('example_nft');
    validate('move_package');
  });
});

function validate(
  key: 'coin' | 'example_nft' | 'move_package'
): GetObjectInfoResponse {
  const data = mockObjectData[key];
  expect(isGetObjectInfoResponse(data)).toBeTruthy();
  return data as GetObjectInfoResponse;
}
