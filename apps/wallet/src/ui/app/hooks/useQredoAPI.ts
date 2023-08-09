// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';

import { useBackgroundClient } from './useBackgroundClient';
import { useQredoInfo } from './useQredoInfo';
import { QredoAPI } from '_src/shared/qredo-api';

const API_INSTANCES: Record<string, QredoAPI> = {};

export function useQredoAPI(qredoID?: string) {
	const backgroundClient = useBackgroundClient();
	const { data, isLoading, error } = useQredoInfo(qredoID ? { qredoID } : null);
	const [api, setAPI] = useState(() => (qredoID && API_INSTANCES[qredoID]) || null);
	useEffect(() => {
		if (data?.qredoInfo?.apiUrl && data?.qredoInfo?.accessToken && qredoID) {
			const instance = API_INSTANCES[qredoID];
			if (instance && instance.accessToken !== data.qredoInfo.accessToken) {
				instance.accessToken = data.qredoInfo.accessToken;
			} else if (!instance) {
				API_INSTANCES[qredoID] = new QredoAPI(qredoID, data.qredoInfo.apiUrl, {
					accessTokenRenewalFN: async (qredoID) =>
						(await backgroundClient.getQredoConnectionInfo({ qredoID }, true)).qredoInfo
							?.accessToken || null,
					accessToken: data.qredoInfo.accessToken,
				});
			}
		}
		setAPI((qredoID && API_INSTANCES[qredoID]) || null);
	}, [backgroundClient, data?.qredoInfo?.apiUrl, data?.qredoInfo?.accessToken, qredoID]);
	return [api, isLoading, error] as const;
}
