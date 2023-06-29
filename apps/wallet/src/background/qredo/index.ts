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
import {
	type UIQredoInfo,
	type QredoConnectPendingRequest,
	type QredoConnectIdentity,
} from './types';
import { qredoConnectPageUrl, toUIQredoPendingRequest, validateInputOrThrow } from './utils';
import Tabs from '../Tabs';
import { Window } from '../Window';
import { type ContentScriptConnection } from '../connections/ContentScriptConnection';
import keyring from '../keyring';
import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';
import { type Message } from '_src/shared/messaging/messages';
import { type QredoConnectPayload } from '_src/shared/messaging/messages/payloads/QredoConnect';
import { type AccessTokenResponse, QredoAPI } from '_src/shared/qredo-api';

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

const IN_PROGRESS_ACCESS_TOKENS_RENEWALS: Record<string, Promise<AccessTokenResponse> | null> = {};

async function renewAccessToken(
	requestID: string,
	apiUrl: string,
	refreshToken: string,
	isPendingRequest: boolean,
) {
	let accessToken: string;
	if (!IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID]) {
		IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID] = new QredoAPI(requestID, apiUrl)
			.createAccessToken({ refreshToken })
			.finally(() => (IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID] = null));
		accessToken = (await IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID])!.access_token;
		if (isPendingRequest) {
			await updatePendingRequest(requestID, { accessToken });
		} else {
			await storeQredoConnectionAccessToken(requestID, accessToken);
		}
	} else {
		accessToken = (await IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID])!.access_token;
	}
	return accessToken;
}

// This function returns the connection info for the UI and creates an access token when it doesn't exist or if is forced to be created.
// Because pending and existing connections never have the same ID this function fetches data for either of them based on the id.
export async function getUIQredoInfo(
	filter: { qredoID: string } | { identity: QredoConnectIdentity },
	forceRenewAccessToken: boolean,
): Promise<UIQredoInfo | null> {
	const filterAdj = 'qredoID' in filter ? filter.qredoID : filter.identity;
	const pendingRequest = await getPendingRequest(filterAdj);
	const existingConnection = await getQredoConnection(filterAdj);
	if (!pendingRequest && !existingConnection) {
		return null;
	}
	const { id, service, apiUrl } = (pendingRequest || existingConnection)!;
	const refreshToken = pendingRequest?.token || (await keyring.getQredoRefreshToken(id));
	let accessToken = pendingRequest?.accessToken || existingConnection?.accessToken || null;
	if (forceRenewAccessToken || !accessToken) {
		if (!refreshToken) {
			return null;
		}
		accessToken = await renewAccessToken(id, apiUrl, refreshToken, !!pendingRequest);
	}
	return {
		id,
		service,
		apiUrl,
		accessToken: accessToken,
		accounts: existingConnection?.accounts || [],
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
	// make sure we replace an existing connection when it's the same
	const existingConnection = await getQredoConnection({
		apiUrl,
		origin,
		service,
		organization,
	});
	const qredoIDToUse = existingConnection?.id || qredoID;
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
	await deletePendingRequest(pendingRequest);
	qredoEvents.emit('onConnectionResponse', {
		allowed: true,
		request: pendingRequest,
	});
}
