// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type SuiNetwork } from '~/components/module/SuiNetwork';

export interface SuiVerificationCheckResultDto {
	network: SuiNetwork;
	packageId: string;
	isVerified: boolean;
	verifiedSrcUrl: string | null;
	errMsg: string | null;
}
