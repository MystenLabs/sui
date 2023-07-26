// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mitt from 'mitt';

import {
	createPendingRequest,
	deletePendingRequest,
	getAllPendingRequests,
	getPendingRequest,
	getQredoConnection,
	storeAllPendingRequests,
	storeQredoConnection,
	storeQredoConnectionAccessToken,
	updatePendingRequest,
} from './storage';
import { type UIQredoInfo, type QredoConnectPendingRequest } from './types';
import { qredoConnectPageUrl, toUIQredoPendingRequest, validateInputOrThrow } from './utils';
import Tabs from '../Tabs';
import { Window } from '../Window';
import { getQredoAccountSource } from '../account-sources';
import { QredoAccountSource } from '../account-sources/QredoAccountSource';
import { addNewAccounts } from '../accounts';
import { type QredoSerializedAccount } from '../accounts/QredoAccount';
import { type ContentScriptConnection } from '../connections/ContentScriptConnection';
import keyring from '../keyring';
import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';
import { NEW_ACCOUNTS_ENABLED } from '_src/shared/constants';
import { type Message } from '_src/shared/messaging/messages';
import { type QredoConnectPayload } from '_src/shared/messaging/messages/payloads/QredoConnect';
import { QredoAPI } from '_src/shared/qredo-api';

const qredoEvents = mitt<{
	onConnectionResponse: {
		allowed: boolean;
		request: QredoConnectPendingRequest;
	};
}>();

export const onQredoEvent = qredoEvents.on;
export const offQredoEvent = qredoEvents.off;

export async function requestUserApproval(
	input: QredoConnectInput,
	connection: ContentScriptConnection,
	message: Message,
) {
	const origin = connection.origin;
	const { service, apiUrl, token, organization } = validateInputOrThrow(input);
	const connectionIdentity = {
		service,
		apiUrl,
		origin,
		organization,
	};
	const existingPendingRequest = await getPendingRequest(connectionIdentity);
	if (existingPendingRequest) {
		const qredoConnectUrl = qredoConnectPageUrl(existingPendingRequest.id);
		const changes: Parameters<typeof updatePendingRequest>['1'] = {
			messageID: message.id,
			append: true,
			token: token,
		};
		if (
			!(await Tabs.highlight({
				url: qredoConnectUrl,
				windowID: existingPendingRequest.windowID || undefined,
				match: ({ url, inAppRedirectUrl }) => {
					const urlMatch = `/dapp/qredo-connect/${existingPendingRequest.id}`;
					return (
						url.includes(urlMatch) || (!!inAppRedirectUrl && inAppRedirectUrl.includes(urlMatch))
					);
				},
			}))
		) {
			const approvalWindow = new Window(qredoConnectUrl);
			await approvalWindow.show();
			if (approvalWindow.id) {
				changes.windowID = approvalWindow.id;
			}
		}
		await updatePendingRequest(existingPendingRequest.id, changes);
		return;
	}
	const request = await createPendingRequest(
		{
			service,
			apiUrl,
			token,
			origin,
			originFavIcon: connection.originFavIcon,
			accessToken: null,
			organization,
		},
		message.id,
	);
	const approvalWindow = new Window(qredoConnectPageUrl(request.id));
	await approvalWindow.show();
	if (approvalWindow.id) {
		await updatePendingRequest(request.id, { windowID: approvalWindow.id });
	}
}

export async function handleOnWindowClosed(windowID: number) {
	const allRequests = await getAllPendingRequests();
	const remainingRequests: QredoConnectPendingRequest[] = [];
	allRequests.forEach((aRequest) => {
		if (aRequest.windowID === windowID) {
			qredoEvents.emit('onConnectionResponse', {
				allowed: false,
				request: aRequest,
			});
		} else {
			remainingRequests.push(aRequest);
		}
	});
	if (allRequests.length !== remainingRequests.length) {
		await storeAllPendingRequests(remainingRequests);
	}
}

export async function getUIQredoPendingRequest(requestID: string) {
	const pendingRequest = await getPendingRequest(requestID);
	if (pendingRequest) {
		return toUIQredoPendingRequest(pendingRequest);
	}
	return null;
}

const IN_PROGRESS_ACCESS_TOKENS_RENEWALS: Record<string, Promise<string> | null> = {};

async function renewAccessToken(
	qredoInfo: { requestID: string; apiUrl: string; refreshToken: string } | QredoAccountSource,
	isPendingRequest: boolean,
) {
	let accessToken: string;
	const requestID = qredoInfo instanceof QredoAccountSource ? qredoInfo.id : qredoInfo.requestID;
	if (!IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID]) {
		IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID] =
			qredoInfo instanceof QredoAccountSource
				? qredoInfo.renewAccessToken()
				: new QredoAPI(requestID, qredoInfo.apiUrl)
						.createAccessToken({ refreshToken: qredoInfo.refreshToken })
						.then(({ access_token }) => access_token)
						.finally(() => (IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID] = null));
		accessToken = (await IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID])!;
		if (isPendingRequest) {
			await updatePendingRequest(requestID, { accessToken });
		} else {
			if (!(qredoInfo instanceof QredoAccountSource)) {
				await storeQredoConnectionAccessToken(requestID, accessToken);
			}
		}
	} else {
		accessToken = (await IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID])!;
	}
	return accessToken;
}

// This function returns the connection info for the UI and creates an access token when it doesn't exist or if is forced to be created.
// Because pending and existing connections never have the same ID this function fetches data for either of them based on the id.
export async function getUIQredoInfo(
	qredoID: string,
	forceRenewAccessToken: boolean,
): Promise<UIQredoInfo | null> {
	const pendingRequest = await getPendingRequest(qredoID);
	const existingConnection = pendingRequest
		? null
		: await (NEW_ACCOUNTS_ENABLED ? getQredoAccountSource(qredoID) : getQredoConnection(qredoID));
	if (!pendingRequest && !existingConnection) {
		return null;
	}
	const { id, service, apiUrl } = (pendingRequest || existingConnection)!;
	let refreshToken = pendingRequest?.token || null;
	if (!refreshToken) {
		if (NEW_ACCOUNTS_ENABLED && existingConnection instanceof QredoAccountSource) {
			refreshToken = await existingConnection.refreshToken;
		} else {
			refreshToken = await keyring.getQredoRefreshToken(id);
		}
	}
	let accessToken = pendingRequest?.accessToken || existingConnection?.accessToken || null;
	if (forceRenewAccessToken || !accessToken) {
		if (!refreshToken) {
			return null;
		}
		accessToken = await renewAccessToken(
			existingConnection instanceof QredoAccountSource
				? existingConnection
				: { requestID: id, apiUrl: await apiUrl, refreshToken },
			!!pendingRequest,
		);
	}
	return {
		id,
		service: await service,
		apiUrl: await apiUrl,
		accessToken: await accessToken,
	};
}

export async function rejectQredoConnection({
	qredoID,
}: QredoConnectPayload<'rejectQredoConnection'>['args']) {
	const pendingRequest = await getPendingRequest(qredoID);
	if (pendingRequest) {
		await deletePendingRequest(pendingRequest);
		qredoEvents.emit('onConnectionResponse', {
			allowed: false,
			request: pendingRequest,
		});
	}
}

export async function acceptQredoConnection({
	qredoID,
	password,
	accounts,
}: QredoConnectPayload<'acceptQredoConnection'>['args']) {
	const pendingRequest = await getPendingRequest(qredoID);
	if (!pendingRequest) {
		throw new Error(`Accepting Qredo connection failed, pending request ${qredoID} not found`);
	}
	const { apiUrl, origin, originFavIcon, service, organization } = pendingRequest;
	const connectionIdentity = {
		apiUrl,
		origin,
		service,
		organization,
	};
	if (NEW_ACCOUNTS_ENABLED) {
		// make sure we replace an existing connection when it's the same
		let qredoAccountSource = await getQredoAccountSource(connectionIdentity);
		if (!qredoAccountSource) {
			qredoAccountSource = await QredoAccountSource.createNew({
				password,
				apiUrl,
				origin,
				organization,
				refreshToken: pendingRequest.token,
				service,
			});
		}
		if (!(await qredoAccountSource.isLocked())) {
			// credentials are kept in session storage, force renewal
			await qredoAccountSource.unlock(password);
		}
		const newQredoAccounts: Omit<QredoSerializedAccount, 'id'>[] = [];
		for (const aWallet of accounts) {
			newQredoAccounts.push({
				...aWallet,
				type: 'qredo',
				sourceID: qredoAccountSource.id,
				storageEntityType: 'account-entity',
			});
		}
		await addNewAccounts(newQredoAccounts);
	} else {
		// make sure we replace an existing connection when it's the same
		const existingConnection = await getQredoConnection(connectionIdentity);
		const qredoIDToUse = existingConnection?.id || pendingRequest.id;
		await keyring.storeQredoConnection(qredoIDToUse, pendingRequest.token, password, accounts);
		await storeQredoConnection({
			id: qredoIDToUse,
			apiUrl,
			origin,
			originFavIcon,
			service,
			accounts,
			accessToken: null,
			organization,
		});
	}
	await deletePendingRequest(pendingRequest);
	qredoEvents.emit('onConnectionResponse', {
		allowed: true,
		request: pendingRequest,
	});
}
