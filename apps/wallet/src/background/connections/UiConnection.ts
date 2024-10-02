// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createMessage } from '_messages';
import type { Message } from '_messages';
import type { PortChannelName } from '_messaging/PortChannelName';
import { isBasePayload, type ErrorPayload } from '_payloads';
import type { LoadedFeaturesPayload } from '_payloads/feature-gating';
import { isSetNetworkPayload, type SetNetworkPayload } from '_payloads/network';
import { isGetPermissionRequests, isPermissionResponse } from '_payloads/permissions';
import type { Permission, PermissionRequests } from '_payloads/permissions';
import { isDisconnectApp } from '_payloads/permissions/DisconnectApp';
import type { UpdateActiveOrigin } from '_payloads/tabs/updateActiveOrigin';
import type { ApprovalRequest } from '_payloads/transactions/ApprovalRequest';
import { isGetTransactionRequests } from '_payloads/transactions/ui/GetTransactionRequests';
import type { GetTransactionRequestsResponse } from '_payloads/transactions/ui/GetTransactionRequestsResponse';
import { isTransactionRequestResponse } from '_payloads/transactions/ui/TransactionRequestResponse';
import Permissions from '_src/background/Permissions';
import Tabs from '_src/background/Tabs';
import Transactions from '_src/background/Transactions';
import { growthbook } from '_src/shared/experimentation/features';
import {
	isMethodPayload,
	type MethodPayload,
	type UIAccessibleEntityType,
} from '_src/shared/messaging/messages/payloads/MethodPayload';
import {
	isQredoConnectPayload,
	type QredoConnectPayload,
} from '_src/shared/messaging/messages/payloads/QredoConnect';
import { toEntropy } from '_src/shared/utils/bip39';
import Dexie from 'dexie';
import { BehaviorSubject, filter, switchMap, takeUntil } from 'rxjs';
import Browser from 'webextension-polyfill';
import type { Runtime } from 'webextension-polyfill';

import {
	accountSourcesHandleUIMessage,
	getAccountSourceByID,
	getAllSerializedUIAccountSources,
} from '../account-sources';
import { accountSourcesEvents } from '../account-sources/events';
import { MnemonicAccountSource } from '../account-sources/MnemonicAccountSource';
import { accountsHandleUIMessage, getAllSerializedUIAccounts } from '../accounts';
import { type AccountType } from '../accounts/Account';
import { accountsEvents } from '../accounts/events';
import { getAutoLockMinutes, notifyUserActive, setAutoLockMinutes } from '../auto-lock-accounts';
import { backupDB, getDB, settingsKeys } from '../db';
import { clearStatus, doMigration, getStatus } from '../legacy-accounts/storage-migration';
import NetworkEnv from '../NetworkEnv';
import {
	acceptQredoConnection,
	getUIQredoInfo,
	getUIQredoPendingRequest,
	rejectQredoConnection,
} from '../qredo';
import { Connection } from './Connection';

export class UiConnection extends Connection {
	public static readonly CHANNEL: PortChannelName = 'sui_ui<->background';
	private uiAppInitialized: BehaviorSubject<boolean> = new BehaviorSubject(false);

	constructor(port: Runtime.Port) {
		super(port);
		this.uiAppInitialized
			.pipe(
				filter((init) => init),
				switchMap(() => Tabs.activeOrigin),
				takeUntil(this.onDisconnect),
			)
			.subscribe(({ origin, favIcon }) => {
				this.send(
					createMessage<UpdateActiveOrigin>({
						type: 'update-active-origin',
						origin,
						favIcon,
					}),
				);
			});
	}

	public async notifyEntitiesUpdated(entitiesType: UIAccessibleEntityType) {
		this.send(
			createMessage<MethodPayload<'entitiesUpdated'>>({
				type: 'method-payload',
				method: 'entitiesUpdated',
				args: {
					type: entitiesType,
				},
			}),
		);
	}

	protected async handleMessage(msg: Message) {
		const { payload, id } = msg;
		try {
			if (isGetPermissionRequests(payload)) {
				this.sendPermissions(Object.values(await Permissions.getPermissions()), id);
				// TODO: we should depend on a better message to know if app is initialized
				if (!this.uiAppInitialized.value) {
					this.uiAppInitialized.next(true);
				}
			} else if (isPermissionResponse(payload)) {
				Permissions.handlePermissionResponse(payload);
			} else if (isTransactionRequestResponse(payload)) {
				Transactions.handleMessage(payload);
			} else if (isGetTransactionRequests(payload)) {
				this.sendTransactionRequests(
					Object.values(await Transactions.getTransactionRequests()),
					id,
				);
			} else if (isDisconnectApp(payload)) {
				await Permissions.delete(payload.origin, payload.specificAccounts);
				this.send(createMessage({ type: 'done' }, id));
			} else if (isBasePayload(payload) && payload.type === 'get-features') {
				await growthbook.loadFeatures();
				this.send(
					createMessage<LoadedFeaturesPayload>(
						{
							type: 'features-response',
							features: growthbook.getFeatures(),
							attributes: growthbook.getAttributes(),
						},
						id,
					),
				);
			} else if (isBasePayload(payload) && payload.type === 'get-network') {
				this.send(
					createMessage<SetNetworkPayload>(
						{
							type: 'set-network',
							network: await NetworkEnv.getActiveNetwork(),
						},
						id,
					),
				);
			} else if (isSetNetworkPayload(payload)) {
				await NetworkEnv.setActiveNetwork(payload.network);
				this.send(createMessage({ type: 'done' }, id));
			} else if (isQredoConnectPayload(payload, 'getPendingRequest')) {
				this.send(
					createMessage<QredoConnectPayload<'getPendingRequestResponse'>>(
						{
							type: 'qredo-connect',
							method: 'getPendingRequestResponse',
							args: {
								request: await getUIQredoPendingRequest(payload.args.requestID),
							},
						},
						msg.id,
					),
				);
			} else if (isQredoConnectPayload(payload, 'getQredoInfo')) {
				this.send(
					createMessage<QredoConnectPayload<'getQredoInfoResponse'>>(
						{
							type: 'qredo-connect',
							method: 'getQredoInfoResponse',
							args: {
								qredoInfo: await getUIQredoInfo(
									payload.args.qredoID,
									payload.args.refreshAccessToken,
								),
							},
						},
						msg.id,
					),
				);
			} else if (isQredoConnectPayload(payload, 'acceptQredoConnection')) {
				this.send(
					createMessage<QredoConnectPayload<'acceptQredoConnectionResponse'>>(
						{
							type: 'qredo-connect',
							method: 'acceptQredoConnectionResponse',
							args: { accounts: await acceptQredoConnection(payload.args) },
						},
						id,
					),
				);
			} else if (isQredoConnectPayload(payload, 'rejectQredoConnection')) {
				await rejectQredoConnection(payload.args);
				this.send(createMessage({ type: 'done' }, id));
			} else if (isMethodPayload(payload, 'getStoredEntities')) {
				const entities = await this.getUISerializedEntities(payload.args.type);
				this.send(
					createMessage<MethodPayload<'storedEntitiesResponse'>>(
						{
							method: 'storedEntitiesResponse',
							type: 'method-payload',
							args: {
								type: payload.args.type,
								entities,
							},
						},
						msg.id,
					),
				);
			} else if (await accountSourcesHandleUIMessage(msg, this)) {
				return;
			} else if (await accountsHandleUIMessage(msg, this)) {
				return;
			} else if (isMethodPayload(payload, 'getStorageMigrationStatus')) {
				this.send(
					createMessage<MethodPayload<'storageMigrationStatus'>>(
						{
							method: 'storageMigrationStatus',
							type: 'method-payload',
							args: {
								status: await getStatus(),
							},
						},
						id,
					),
				);
			} else if (isMethodPayload(payload, 'doStorageMigration')) {
				await doMigration(payload.args.password);
				this.send(createMessage({ type: 'done' }, id));
			} else if (isMethodPayload(payload, 'clearWallet')) {
				await Browser.storage.local.clear();
				await Browser.storage.local.set({
					v: -1,
				});
				clearStatus();
				const db = await getDB();
				await db.delete();
				await db.open();
				// prevents future run of auto backup process of the db (we removed everything nothing to backup after logout)
				await db.settings.put({ setting: settingsKeys.isPopulated, value: true });
				this.send(createMessage({ type: 'done' }, id));
			} else if (isMethodPayload(payload, 'getAutoLockMinutes')) {
				await this.send(
					createMessage<MethodPayload<'getAutoLockMinutesResponse'>>(
						{
							type: 'method-payload',
							method: 'getAutoLockMinutesResponse',
							args: { minutes: await getAutoLockMinutes() },
						},
						msg.id,
					),
				);
			} else if (isMethodPayload(payload, 'setAutoLockMinutes')) {
				await setAutoLockMinutes(payload.args.minutes);
				await this.send(createMessage({ type: 'done' }, msg.id));
				return true;
			} else if (isMethodPayload(payload, 'notifyUserActive')) {
				await notifyUserActive();
				await this.send(createMessage({ type: 'done' }, msg.id));
				return true;
			} else if (isMethodPayload(payload, 'resetPassword')) {
				const { password, recoveryData } = payload.args;
				if (!recoveryData.length) {
					throw new Error('Missing recovery data');
				}
				for (const { accountSourceID, entropy } of recoveryData) {
					const accountSource = await getAccountSourceByID(accountSourceID);
					if (!accountSource) {
						throw new Error('Account source not found');
					}
					if (!(accountSource instanceof MnemonicAccountSource)) {
						throw new Error('Invalid account source type');
					}
					await accountSource.verifyRecoveryData(entropy);
				}
				const db = await getDB();
				const zkLoginType: AccountType = 'zkLogin';
				const accountSourceIDs = recoveryData.map(({ accountSourceID }) => accountSourceID);
				await db.transaction('rw', db.accountSources, db.accounts, async () => {
					await db.accountSources.where('id').noneOf(accountSourceIDs).delete();
					await db.accounts
						.where('type')
						.notEqual(zkLoginType)
						.filter(
							(anAccount) =>
								!('sourceID' in anAccount) ||
								typeof anAccount.sourceID !== 'string' ||
								!accountSourceIDs.includes(anAccount.sourceID),
						)
						.delete();
					for (const { accountSourceID, entropy } of recoveryData) {
						await db.accountSources.update(accountSourceID, {
							encryptedData: await Dexie.waitFor(
								MnemonicAccountSource.createEncryptedData(toEntropy(entropy), password),
							),
						});
					}
				});
				await backupDB();
				accountSourcesEvents.emit('accountSourcesChanged');
				accountsEvents.emit('accountsChanged');
				await this.send(createMessage({ type: 'done' }, msg.id));
			} else {
				throw new Error(
					`Unhandled message ${msg.id}. (${JSON.stringify(
						'error' in payload ? `${payload.code}-${payload.message}` : payload.type,
					)})`,
				);
			}
		} catch (e) {
			this.send(
				createMessage<ErrorPayload>(
					{
						error: true,
						code: -1,
						message: (e as Error).message,
					},
					id,
				),
			);
		}
	}

	private sendPermissions(permissions: Permission[], requestID: string) {
		this.send(
			createMessage<PermissionRequests>(
				{
					type: 'permission-request',
					permissions,
				},
				requestID,
			),
		);
	}

	private sendTransactionRequests(txRequests: ApprovalRequest[], requestID: string) {
		this.send(
			createMessage<GetTransactionRequestsResponse>(
				{
					type: 'get-transaction-requests-response',
					txRequests,
				},
				requestID,
			),
		);
	}

	private getUISerializedEntities(type: UIAccessibleEntityType) {
		switch (type) {
			case 'accounts': {
				return getAllSerializedUIAccounts();
			}
			case 'accountSources': {
				return getAllSerializedUIAccountSources();
			}
			default: {
				throw new Error(`Unknown entity type ${type}`);
			}
		}
	}
}
