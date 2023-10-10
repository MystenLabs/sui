// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type SuiNetwork } from '~/components/module/SuiNetwork';
import { type SuiVerificationModuleDto } from '~/components/module/dto/verification/SuiVerificationModuleDto';

export interface SuiVerificationResultDto {
	network: SuiNetwork;
	packageId: string;
	isVerified: boolean;
	verifiedSrcUrl: string | null;
	errMsg: string | null;
	modules: SuiVerificationModuleDto[];
}
