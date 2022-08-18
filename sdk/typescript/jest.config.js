// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/** @type {import('@jest/types').Config.InitialOptions} */
const config = {
  testEnvironment: 'node',
  testMatch: ['<rootDir>/test/**/*.(spec|test).{ts,tsx,js,jsx}'],
};

module.exports = config;
