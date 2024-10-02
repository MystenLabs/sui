// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { openInNewTab } from '_shared/utils';
import { growthbook, setAttributes } from '_src/shared/experimentation/features';
import { coerce, lte } from 'semver';
import Browser from 'webextension-polyfill';

import { lockAllAccountSources } from './account-sources';
import { accountSourcesEvents } from './account-sources/events';
import { getAccountsStatusData, getAllAccounts, lockAllAccounts } from './accounts';
import { accountsEvents } from './accounts/events';
import Alarms, { autoLockAlarmName, cleanUpAlarmName } from './Alarms';
import { Connections } from './connections';
import NetworkEnv from './NetworkEnv';
import Permissions from './Permissions';
import * as Qredo from './qredo';
import { initSentry } from './sentry';
import Transactions from './Transactions';

growthbook.loadFeatures().catch(() => {
	// silence the error
});
initSentry();

Browser.runtime.onInstalled.addListener(async ({ reason, previousVersion }) => {
	// Skip automatically opening the onboarding in end-to-end tests.
	if (navigator.userAgent === 'Playwright') {
		return;
	}
	Alarms.setCleanUpAlarm();
	// TODO: Our versions don't use semver, and instead are date-based. Instead of using the semver
	// library, we can use some combination of parsing into a date + inspecting patch.
	const previousVersionSemver = coerce(previousVersion)?.version;
	if (reason === 'install') {
		await Browser.storage.local.set({
			v: -1,
		});
		openInNewTab();
	} else if (reason === 'update' && previousVersionSemver && lte(previousVersionSemver, '0.1.1')) {
		// clear everything in the storage
		// mainly done to clear the mnemonic that was stored
		// as plain text
		await Browser.storage.local.clear();
		await Browser.storage.local.set({
			v: -1,
		});
	} else if (reason === 'update') {
		const storageVersion = (await Browser.storage.local.get({ v: null })).v;
		// handle address size update and include storage version
		if (storageVersion === null) {
			//clear permissions and active_account because currently they are using the previous address size
			await Browser.storage.local.set({
				permissions: {},
				active_account: null,
				v: -1,
			});
		}
	}
});

const connections = new Connections();

Permissions.permissionReply.subscribe((permission) => {
	if (permission) {
		connections.notifyContentScript({
			event: 'permissionReply',
			permission,
		});
	}
});

Permissions.on('connectedAccountsChanged', async ({ origin, accounts }) => {
	connections.notifyContentScript({
		event: 'walletStatusChange',
		origin,
		change: {
			accounts: await getAccountsStatusData(accounts),
		},
	});
});

accountsEvents.on('accountsChanged', async () => {
	connections.notifyUI({ event: 'storedEntitiesUpdated', type: 'accounts' });
	await Permissions.ensurePermissionAccountsUpdated(
		await Promise.all(
			(await getAllAccounts()).map(async (anAccount) => ({ address: await anAccount.address })),
		),
	);
});
accountsEvents.on('accountStatusChanged', () => {
	connections.notifyUI({ event: 'storedEntitiesUpdated', type: 'accounts' });
});
accountsEvents.on('activeAccountChanged', () => {
	connections.notifyUI({ event: 'storedEntitiesUpdated', type: 'accounts' });
});
accountSourcesEvents.on('accountSourceStatusUpdated', () => {
	connections.notifyUI({ event: 'storedEntitiesUpdated', type: 'accountSources' });
});
accountSourcesEvents.on('accountSourcesChanged', () => {
	connections.notifyUI({ event: 'storedEntitiesUpdated', type: 'accountSources' });
});

Browser.alarms.onAlarm.addListener((alarm) => {
	if (alarm.name === autoLockAlarmName) {
		lockAllAccounts();
		lockAllAccountSources();
	} else if (alarm.name === cleanUpAlarmName) {
		Transactions.clearStaleTransactions();
	}
});

NetworkEnv.getActiveNetwork().then(async ({ env, customRpcUrl }) => {
	setAttributes({
		apiEnv: env,
		customRPC: customRpcUrl,
	});
});

NetworkEnv.on('changed', async (network) => {
	setAttributes({
		apiEnv: network.env,
		customRPC: network.customRpcUrl,
	});
	connections.notifyUI({ event: 'networkChanged', network });
	connections.notifyContentScript({
		event: 'walletStatusChange',
		change: { network },
	});
});

Browser.windows.onRemoved.addListener(async (id) => {
	await Qredo.handleOnWindowClosed(id);
});

Qredo.onQredoEvent('onConnectionResponse', ({ allowed, request }) => {
	request.messageIDs.forEach((aMessageID) => {
		connections.notifyContentScript(
			{
				event: 'qredoConnectResult',
				origin: request.origin,
				allowed,
			},
			aMessageID,
		);
	});
});
