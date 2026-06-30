// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export default {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/test/**/*.test.ts'],
  testTimeout: 120000, // 10000 ms = 10 seconds
  globals: { fetch: global.fetch },
  transform: {
    '^.+\\.tsx?$': [
      'ts-jest',
      {
        tsconfig: '<rootDir>/test/tsconfig.test.json',
      },
    ],
  },
};
