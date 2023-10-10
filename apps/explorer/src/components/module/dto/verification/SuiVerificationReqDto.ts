// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type SuiNetwork } from '~/components/module/SuiNetwork';

export interface SuiVerificationReqDto {
	network: SuiNetwork;
	packageId: string;
	srcFileId: string;
}
