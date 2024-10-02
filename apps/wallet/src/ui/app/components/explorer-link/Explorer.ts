// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DEFAULT_API_ENV } from '_app/ApiProvider';
import { getUrlWithDeviceId } from '_src/shared/analytics/amplitude';
import { API_ENV } from '_src/shared/api-env';

const API_ENV_TO_EXPLORER_ENV: Record<API_ENV, string | undefined> = {
	[API_ENV.local]: 'local',
	[API_ENV.devNet]: 'devnet',
	[API_ENV.testNet]: 'testnet',
	[API_ENV.mainnet]: 'mainnet',
	[API_ENV.customRPC]: '',
};

const EXPLORER_LINK = 'https://suiexplorer.com/';

//TODO - this is a temporary solution, we should have a better way to get the explorer url
function getExplorerUrl(path: string, apiEnv: API_ENV = DEFAULT_API_ENV, customRPC: string) {
	const explorerEnv = apiEnv === 'customRPC' ? customRPC : API_ENV_TO_EXPLORER_ENV[apiEnv];

	const url = getUrlWithDeviceId(new URL(path, EXPLORER_LINK));
	if (explorerEnv) {
		url.searchParams.append('network', explorerEnv);
	}

	return url.href;
}

export function getObjectUrl(
	objectID: string,
	apiEnv: API_ENV,
	customRPC: string,
	moduleName?: string | null,
) {
	return getExplorerUrl(
		`/object/${objectID}${moduleName ? `?module=${moduleName}` : ''}`,
		apiEnv,
		customRPC,
	);
}

export function getTransactionUrl(txDigest: string, apiEnv: API_ENV, customRPC: string) {
	return getExplorerUrl(`/txblock/${encodeURIComponent(txDigest)}`, apiEnv, customRPC);
}

export function getAddressUrl(address: string, apiEnv: API_ENV, customRPC: string) {
	return getExplorerUrl(`/address/${address}`, apiEnv, customRPC);
}

export function getValidatorUrl(address: string, apiEnv: API_ENV, customRPC: string) {
	return getExplorerUrl(`/validator/${address}`, apiEnv, customRPC);
}
