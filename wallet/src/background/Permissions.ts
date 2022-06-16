import { filter, lastValueFrom, map, race, Subject, take, tap } from 'rxjs';
import { v4 as uuidV4 } from 'uuid';
import Browser from 'webextension-polyfill';

import { Window } from './Window';

import type { ContentScriptConnection } from './connections/ContentScriptConnection';
import type {
    Permission,
    PermissionResponse,
    PermissionType,
} from '_messages/payloads/permissions';

function openPermissionWindow(permissionID: string) {
    return new Window(
        Browser.runtime.getURL('ui.html') +
            `#/connect/${encodeURIComponent(permissionID)}`
    );
}

const PERMISSIONS_STORAGE_KEY = 'permissions';

class Permissions {
    // TODO: move pending to storage or memory
    private _pending: Map<string, PermissionType> = new Map();
    private _permissionResponses: Subject<PermissionResponse> = new Subject();

    public async acquirePermission(
        permissionType: PermissionType,
        connection: ContentScriptConnection
    ): Promise<Permission> {
        const origin = connection.origin;
        if (this._pending.has(origin)) {
            throw new Error('Pending permission request');
        }
        const currentPermissions = await this.getPermissions();
        const existingPermission = currentPermissions.find(
            (aPermission) =>
                aPermission.origin === origin &&
                aPermission.permissions.includes(permissionType)
        );
        if (existingPermission?.allowed) {
            return existingPermission;
        }
        this._pending.set(connection.origin, permissionType);
        const pRequest = await this.createPermissionRequest(
            connection.origin,
            permissionType,
            existingPermission
        );
        const permissionWindow = openPermissionWindow(pRequest.id);
        const onWindowCloseStream = await permissionWindow.show();
        const responseStream = this._permissionResponses.pipe(
            filter((resp) => resp.id === pRequest.id),
            map((resp) => {
                pRequest.allowed = resp.allowed;
                pRequest.accounts = resp.accounts;
                pRequest.responseDate = resp.responseDate;
                return pRequest;
            }),
            tap(() => permissionWindow.close())
        );
        return lastValueFrom(
            race(
                onWindowCloseStream.pipe(
                    map(() => {
                        console.log('Window closed rejecting permission');
                        pRequest.allowed = false;
                        pRequest.accounts = [];
                        pRequest.responseDate = new Date().toISOString();
                        return pRequest;
                    })
                ),
                responseStream
            ).pipe(
                take(1),
                tap(async (permission) => {
                    await this.storePermission(permission);
                    this._pending.delete(origin);
                }),
                map((permission) => {
                    if (!permission.allowed) {
                        throw new Error('Permission rejected');
                    }
                    return permission;
                })
            )
        );
    }

    public handlePermissionResponse(response: PermissionResponse) {
        this._permissionResponses.next(response);
    }

    public async getPermissions(): Promise<Permission[]> {
        return (
            await Browser.storage.local.get({ [PERMISSIONS_STORAGE_KEY]: [] })
        )[PERMISSIONS_STORAGE_KEY];
    }

    private async createPermissionRequest(
        origin: string,
        permissionType: PermissionType,
        existingPermission?: Permission
    ): Promise<Permission> {
        if (existingPermission) {
            // TODO: what if it has more permissions?
            existingPermission.allowed = null;
            existingPermission.responseDate = null;
            await this.storePermission(existingPermission);
            return existingPermission;
        }
        const permission: Permission = {
            id: uuidV4(),
            accounts: [],
            allowed: null,
            createdDate: new Date().toISOString(),
            origin,
            permissions: [permissionType],
            responseDate: null,
        };
        const permissions = await this.getPermissions();
        permissions.push(permission);
        await Browser.storage.local.set({
            [PERMISSIONS_STORAGE_KEY]: permissions,
        });
        return permission;
    }

    private async storePermission(permission: Permission) {
        const permissions = await this.getPermissions();
        const index = permissions.findIndex(
            (aPermission) => aPermission.id === permission.id
        );
        console.log('storeResponse index', index);
        if (index >= 0) {
            permissions.splice(index, 1, permission);
        }
        await Browser.storage.local.set({
            [PERMISSIONS_STORAGE_KEY]: permissions,
        });
    }
}

export default new Permissions();
