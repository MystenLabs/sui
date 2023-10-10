// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export interface SuiVerificationModuleDto {
	moduleName: string;
	isVerified: boolean;
	onChainByteCode: string | null;
	offChainByteCode: string | null;
}
