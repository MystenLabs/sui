// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type QredoConnectInput } from '_src/dapp-interface/WalletStandardInterface';
import { type Message } from '_src/shared/messaging/messages';
import { type QredoConnectPayload } from '_src/shared/messaging/messages/payloads/QredoConnect';
import { QredoAPI } from '_src/shared/qredo-api';
import mitt from 'mitt';

import { getQredoAccountSource } from '../account-sources';
import { QredoAccountSource } from '../account-sources/QredoAccountSource';
import { addNewAccounts } from '../accounts';
import { type QredoAccount, type QredoSerializedAccount } from '../accounts/QredoAccount';
import { type ContentScriptConnection } from '../connections/ContentScriptConnection';
import Tabs from '../Tabs';
import { Window } from '../Window';
import {
	createPendingRequest,
	deletePendingRequest,
	getAllPendingRequests,
	getPendingRequest,
	storeAllPendingRequests,
	updatePendingRequest,
} from './storage';
import { type QredoConnectPendingRequest, type UIQredoInfo } from './types';
import { qredoConnectPageUrl, toUIQredoPendingRequest, validateInputOrThrow } from './utils';

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
					const urlMatch = `/accounts/qredo-connect/${existingPendingRequest.id}`;
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
) {
	let accessToken: string;
	const isPendingRequest = !(qredoInfo instanceof QredoAccountSource);
	const requestID = isPendingRequest ? qredoInfo.requestID : qredoInfo.id;
	if (!IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID]) {
		IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID] = isPendingRequest
			? new QredoAPI(requestID, qredoInfo.apiUrl)
					.createAccessToken({ refreshToken: qredoInfo.refreshToken })
					.then(({ access_token }) => access_token)
					.finally(() => (IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID] = null))
			: qredoInfo.renewAccessToken();
		accessToken = (await IN_PROGRESS_ACCESS_TOKENS_RENEWALS[requestID])!;
		if (isPendingRequest) {
			await updatePendingRequest(requestID, { accessToken });
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
	const existingConnection = pendingRequest ? null : await getQredoAccountSource(qredoID);
	if (!pendingRequest && !existingConnection) {
		return null;
	}
	const { id, service, apiUrl } = (pendingRequest || existingConnection)!;
	const refreshToken = pendingRequest
		? pendingRequest.token
		: await existingConnection!.refreshToken;
	let accessToken = pendingRequest?.accessToken || existingConnection?.accessToken || null;
	if (forceRenewAccessToken || !accessToken) {
		if (!refreshToken) {
			return null;
		}
		accessToken = await renewAccessToken(
			existingConnection || { requestID: id, apiUrl: await apiUrl, refreshToken },
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
	// make sure we replace an existing connection when it's the same
	let qredoAccountSource = await getQredoAccountSource(connectionIdentity);
	if (!qredoAccountSource) {
		qredoAccountSource = await QredoAccountSource.save(
			await QredoAccountSource.createNew({
				password,
				apiUrl,
				origin,
				organization,
				refreshToken: pendingRequest.token,
				service,
				originFavIcon: originFavIcon || '',
			}),
		);
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
			lastUnlockedOn: null,
			selected: false,
			nickname: null,
			createdAt: Date.now(),
		});
	}
	const connectedAccounts = (await addNewAccounts(newQredoAccounts)) as QredoAccount[];
	await deletePendingRequest(pendingRequest);
	qredoEvents.emit('onConnectionResponse', {
		allowed: true,
		request: pendingRequest,
	});
	return Promise.all(connectedAccounts.map(async (anAccount) => await anAccount.toUISerialized()));
}
