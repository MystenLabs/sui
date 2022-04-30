// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockObjectData from '../mocks/data/objects.json';

import { isGetObjectInfoResponse } from '../../src/index.guard';

describe('Test Objects Definition', () => {
  it('Test against different object definitions', () => {
    const objects = mockObjectData;
    expect(isGetObjectInfoResponse(objects['coin'])).toBeTruthy();
    expect(isGetObjectInfoResponse(objects['example_nft'])).toBeTruthy();
  });
});
