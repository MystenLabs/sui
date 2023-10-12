// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { v4 as uuid } from 'uuid';

import {
	getFromSessionStorage,
	isSessionStorageSupported,
	setToSessionStorage,
} from '../storage-utils';
import type { QredoConnectIdentity, QredoConnectPendingRequest } from './types';
import { isSameQredoConnection } from './utils';

const SESSION_STORAGE_KEY = 'qredo-connect-requests';

function sessionStorageAssert() {
	if (!isSessionStorageSupported()) {
		throw new Error('Session storage is required. Please update your browser');
	}
}

export async function getAllPendingRequests() {
	sessionStorageAssert();
	return (await getFromSessionStorage<QredoConnectPendingRequest[]>(SESSION_STORAGE_KEY, [])) || [];
}

export function storeAllPendingRequests(requests: QredoConnectPendingRequest[]) {
	sessionStorageAssert();
	return setToSessionStorage(SESSION_STORAGE_KEY, requests);
}

export async function getPendingRequest(requestIdentity: QredoConnectIdentity | string) {
	return (
		(await getAllPendingRequests()).find((aRequest) =>
			isSameQredoConnection(requestIdentity, aRequest),
		) || null
	);
}

async function storePendingRequest(request: QredoConnectPendingRequest) {
	const allPendingRequests = await getAllPendingRequests();
	const existingIndex = allPendingRequests.findIndex((aRequest) => aRequest.id === request.id);
	if (existingIndex >= 0) {
		allPendingRequests.splice(existingIndex, 1, request);
	} else {
		allPendingRequests.push(request);
	}
	await storeAllPendingRequests(allPendingRequests);
}

export async function deletePendingRequest(request: QredoConnectPendingRequest) {
	await storeAllPendingRequests(
		(await getAllPendingRequests()).filter(({ id }) => request.id !== id),
	);
}

export async function createPendingRequest(
	options: Omit<QredoConnectPendingRequest, 'id' | 'messageIDs' | 'windowID'>,
	messageID: string,
) {
	const newRequest: QredoConnectPendingRequest = {
		...options,
		id: uuid(),
		windowID: null,
		messageIDs: [messageID],
	};
	await storePendingRequest(newRequest);
	return newRequest;
}

export async function updatePendingRequest(
	id: string,
	change: {
		windowID?: number;
		messageID?: string;
		append?: boolean;
		token?: string;
		accessToken?: string;
	},
) {
	const request = await getPendingRequest(id);
	if (!request) {
		return;
	}
	if (typeof change.windowID === 'number') {
		request.windowID = change.windowID;
	}
	if (change.messageID) {
		if (change.append) {
			request.messageIDs.push(change.messageID);
		} else {
			request.messageIDs = [change.messageID];
		}
	}
	if (change.token) {
		request.token = change.token;
	}
	if (change.accessToken) {
		request.accessToken = change.accessToken;
	}
	await storePendingRequest(request);
}
