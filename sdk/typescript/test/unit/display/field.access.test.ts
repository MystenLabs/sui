// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { getDisplay } from '../../../src/utils/display';

describe('Display Access Test', () => {
  it('Should work with fields and nested fields', () => {

    const test_obj = {
        name: 'Blurp',
        age: 'Beepo',
        meta: {
            haha: 'Yes!'
        }
    };

    const test_file = {
        name: 'Capybara: {name} / {age}',
        age: 'Age is {age} years!',
        whois: 'Meta? {meta.haha}'
    };

    console.log('display', getDisplay(test_file, test_obj));
  });
});
