// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeEach, vi } from 'vitest';
import { RPCValidationError } from '../../src';

beforeEach(() => {
  const originalWarn = console.warn;
  vi.spyOn(console, 'warn').mockImplementation((message) => {
    if (message instanceof RPCValidationError) {
      throw message;
    }

    originalWarn(message);
  });
});
