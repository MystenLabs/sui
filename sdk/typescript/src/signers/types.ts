// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SerializedSignature } from '../cryptography/signature';

export type SignedTransaction = {
  transactionBlockBytes: string;
  signature: SerializedSignature;
};

export type SignedMessage = {
  messageBytes: string;
  signature: SerializedSignature;
};
