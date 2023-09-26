// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ALL_PERMISSION_TYPES, isValidPermissionTypes } from '_payloads/permissions';
import type { Permission, PermissionResponse, PermissionType } from '_payloads/permissions';
import mitt from 'mitt';
import { catchError, concatMap, filter, from, mergeWith, share, Subject } from 'rxjs';
import type { Observable } from 'rxjs';
import { v4 as uuidV4 } from 'uuid';
import Browser from 'webextension-polyfill';

import type { ContentScriptConnection } from './connections/ContentScriptConnection';
import Tabs from './Tabs';
import { Window } from './Window';

const PERMISSIONS_STORAGE_KEY = 'permissions';
const PERMISSION_UI_URL = `${Browser.runtime.getURL('ui.html')}#/dapp/connect/`;
const PERMISSION_UI_URL_REGEX = new RegExp(`${PERMISSION_UI_URL}([0-9a-f-]+$)`, 'i');

type PermissionEvents = {
	connectedAccountsChanged: {
		origin: string;
		accounts: string[];
	};
};

class Permissions {
	#events = mitt<PermissionEvents>();

	public static getUiUrl(permissionID: string) {
		return `${PERMISSION_UI_URL}${encodeURIComponent(permissionID)}`;
	}

	public static isPermissionUiUrl(url: string) {
		return PERMISSION_UI_URL_REGEX.test(url);
	}

	public static getPermissionIDFromUrl(url: string) {
		const match = PERMISSION_UI_URL_REGEX.exec(url);
		if (match) {
			return match[1];
		}
		return null;
	}

	private _permissionResponses: Subject<PermissionResponse> = new Subject();
	//NOTE: we need at least one subscription in order for this to handle permission requests
	public readonly permissionReply: Observable<Permission | null>;

	constructor() {
		this.permissionReply = this._permissionResponses.pipe(
			mergeWith(
				Tabs.onRemoved.pipe(
					filter((aTab) => {
						return (
							Permissions.isPermissionUiUrl(aTab.url || '') && !aTab.nextUrl?.includes('/locked')
						);
					}),
				),
			),
			concatMap((data) =>
				from(
					(async () => {
						let permissionID: string | null;
						const response: Partial<Permission> = {
							allowed: false,
							accounts: [],
							responseDate: new Date().toISOString(),
						};
						if ('url' in data) {
							permissionID = Permissions.getPermissionIDFromUrl(data.url || '');
						} else {
							permissionID = data.id;
							response.allowed = data.allowed;
							response.accounts = data.accounts;
							response.responseDate = data.responseDate;
						}
						let aPermissionRequest: Permission | null = null;
						if (permissionID) {
							aPermissionRequest = await this.getPermissionByID(permissionID);
						}
						if (aPermissionRequest && this.isPendingPermissionRequest(aPermissionRequest)) {
							const finalPermission: Permission = {
								...aPermissionRequest,
								...response,
							};
							return finalPermission;
						}
						// ignore the event
						return null;
					})(),
				).pipe(
					filter((data) => data !== null),
					concatMap((permission) =>
						from(
							(async () => {
								if (permission) {
									await this.storePermission(permission);
									this.#events.emit('connectedAccountsChanged', {
										origin: permission.origin,
										accounts: permission.allowed ? permission.accounts : [],
									});
									return permission;
								}
								return null;
							})(),
						),
					),
				),
			),
			// we ignore any errors and continue to handle other requests
			// TODO: expose those errors to dapp?
			catchError((_error, originalSource) => originalSource),
			share(),
		);
	}

	public async startRequestPermissions(
		permissionTypes: readonly PermissionType[],
		connection: ContentScriptConnection,
		requestMsgID: string,
	): Promise<Permission | null> {
		if (!isValidPermissionTypes(permissionTypes)) {
			throw new Error(
				`Invalid permission types. Allowed type are ${ALL_PERMISSION_TYPES.join(', ')}`,
			);
		}
		const { origin } = connection;
		const existingPermission = await this.getPermission(origin);
		const hasPendingRequest = await this.hasPendingPermissionRequest(origin, existingPermission);
		if (hasPendingRequest) {
			if (existingPermission) {
				const uiUrl = Permissions.getUiUrl(existingPermission.id);
				const found = await Tabs.highlight({ url: uiUrl });
				if (!found) {
					await new Window(uiUrl).show();
				}
			}
			throw new Error('Another permission request is pending.');
		}
		const alreadyAllowed = await this.hasPermissions(origin, permissionTypes, existingPermission);
		if (alreadyAllowed && existingPermission) {
			return existingPermission;
		}
		const pRequest = await this.createPermissionRequest(
			connection.origin,
			permissionTypes,
			connection.originFavIcon,
			requestMsgID,
			connection.pagelink,
			existingPermission,
		);
		await new Window(Permissions.getUiUrl(pRequest.id)).show();
		return null;
	}

	public handlePermissionResponse(response: PermissionResponse) {
		this._permissionResponses.next(response);
	}

	public async getPermissions(): Promise<Record<string, Permission>> {
		return (await Browser.storage.local.get({ [PERMISSIONS_STORAGE_KEY]: {} }))[
			PERMISSIONS_STORAGE_KEY
		];
	}

	public async getPermission(
		origin: string,
		permission?: Permission | null,
	): Promise<Permission | null> {
		if (permission && permission.origin !== origin) {
			throw new Error(
				`Provided permission has different origin from the one provided. "${permission.origin} !== ${origin}"`,
			);
		}
		if (permission) {
			return permission;
		}
		const permissions = await this.getPermissions();
		return permissions[origin] || null;
	}

	public async hasPendingPermissionRequest(
		origin: string,
		permission?: Permission | null,
	): Promise<boolean> {
		const existingPermission = await this.getPermission(origin, permission);
		return !!existingPermission && this.isPendingPermissionRequest(existingPermission);
	}

	public async hasPermissions(
		origin: string,
		permissionTypes: readonly PermissionType[],
		permission?: Permission | null,
		address?: string,
	): Promise<boolean> {
		const existingPermission = await this.getPermission(origin, permission);
		return Boolean(
			existingPermission &&
				existingPermission.allowed &&
				permissionTypes.every((permissionType) =>
					existingPermission.permissions.includes(permissionType),
				) &&
				(!address || (address && existingPermission.accounts.includes(address))),
		);
	}

	public async delete(origin: string, specificAccounts: string[] = []) {
		const allPermissions = await this.getPermissions();
		const thePermission = allPermissions[origin];
		if (thePermission) {
			const remainingAccounts = specificAccounts.length
				? thePermission.accounts.filter((anAccount) => !specificAccounts.includes(anAccount))
				: [];
			if (!remainingAccounts.length) {
				delete allPermissions[origin];
			} else {
				thePermission.accounts = remainingAccounts;
			}
			await Browser.storage.local.set({
				[PERMISSIONS_STORAGE_KEY]: allPermissions,
			});
			this.#events.emit('connectedAccountsChanged', {
				origin,
				accounts: remainingAccounts,
			});
		}
	}

	public async ensurePermissionAccountsUpdated(accounts: { address: string }[]) {
		const allPermissions = await this.getPermissions();
		const availableAccountsIndex = accounts.reduce<Record<string, boolean>>((acc, { address }) => {
			acc[address] = true;
			return acc;
		}, {});
		Object.entries(allPermissions).forEach(async ([origin, { accounts, allowed }]) => {
			if (allowed) {
				const accountsToDisconnect = accounts.filter(
					(anAddress) => !availableAccountsIndex[anAddress],
				);
				if (accountsToDisconnect.length) {
					await this.delete(origin, accountsToDisconnect);
				}
			}
		});
	}

	public on = this.#events.on;

	public off = this.#events.off;

	private async createPermissionRequest(
		origin: string,
		permissionTypes: readonly PermissionType[],
		favIcon: string | undefined,
		requestMsgID: string,
		pagelink?: string | undefined,
		existingPermission?: Permission | null,
	): Promise<Permission> {
		let permissionToStore: Permission;
		if (existingPermission) {
			existingPermission.responseDate = null;
			existingPermission.requestMsgID = requestMsgID;
			if (existingPermission.allowed) {
				permissionTypes.forEach((aPermission) => {
					if (!existingPermission.permissions.includes(aPermission)) {
						existingPermission.permissions.push(aPermission);
					}
				});
			} else {
				existingPermission.permissions = permissionTypes as PermissionType[];
			}
			existingPermission.allowed = null;
			permissionToStore = existingPermission;
		} else {
			permissionToStore = {
				id: uuidV4(),
				accounts: [],
				allowed: null,
				createdDate: new Date().toISOString(),
				origin,
				favIcon,
				pagelink,
				permissions: permissionTypes as PermissionType[],
				responseDate: null,
				requestMsgID,
			};
		}
		await this.storePermission(permissionToStore);
		return permissionToStore;
	}

	private async storePermission(permission: Permission) {
		const permissions = await this.getPermissions();
		permissions[permission.origin] = permission;
		await Browser.storage.local.set({
			[PERMISSIONS_STORAGE_KEY]: permissions,
		});
	}

	private async getPermissionByID(id: string) {
		const permissions = await this.getPermissions();
		for (const aPermission of Object.values(permissions)) {
			if (aPermission.id === id) {
				return aPermission;
			}
		}
		return null;
	}

	private isPendingPermissionRequest(permissionRequest: Permission) {
		return permissionRequest.responseDate === null;
	}
}

const permissions = new Permissions();
export default permissions;
