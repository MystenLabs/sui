// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const references = [
  {
    type: "doc",
    label: "References",
    id: "references",
  },
  {
    type: "category",
    label: "JSON RPC",
    link: {
      type: "doc",
      id: "references/json-rpc/json-rpc-format",
    },
    items: [
      /*{
            type: 'link',
            label: 'API Reference',
            href: '/sui-api'
          },*/
      "references/json-rpc/rpc-api",
    ],
  },
  {
    type: "category",
    label: "Sui CLI",
    link: {
      type: "doc",
      id: "references/cli",
    },
    items: [
      "references/cli/client",
      "references/cli/console",
      "references/cli/keytool",
      "references/cli/move",
      "references/cli/validator",
    ],
  },
  {
    type: "category",
    label: "Sui SDKs",
    link: {
      type: "doc",
      id: "references/sui-sdks",
    },
    items: ["references/sdk/ts-sdk", "references/sdk/rust-sdk"],
  },
  "references/dapp-kit",
  {
    type: "category",
    label: "Sui Move",
    link: {
      type: "doc",
      id: "references/sui-move",
    },
    items: [
      "references/move/move-toml",
      "references/move/move-lock",
      "references/move/language",
    ],
  },
];

module.exports = references;
