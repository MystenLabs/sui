// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const getStarted = [
  'get-started/sui-mindset',
  {
    type: 'category',
    label: 'Get Started Building on Sui',
    collapsed: false,
    link: {
      type: 'doc',
      id: 'get-started',
    },
    items: [
      'get-started/sui-install',
      'get-started/get-address',
      'get-started/get-coins',
      'get-started/connect',
    ],
  },
  'get-started/graphql-rpc',
];

module.exports = getStarted;