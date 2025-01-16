// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { AwsClientOptions } from './aws-client.js';
import type { AwsKmsSignerOptions } from './aws-kms-signer.js';
import { AwsKmsSigner } from './aws-kms-signer.js';

export { AwsKmsSigner };

export type { AwsKmsSignerOptions, AwsClientOptions };
