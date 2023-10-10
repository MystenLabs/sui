// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiNetwork } from '~/components/module/SuiNetwork';

export interface SuiVerificationSrcUploadReqDto {
	network: SuiNetwork;
	packageId: string;
	srcZipFile: File;
}
