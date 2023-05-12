// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bytesToHex, randomBytes } from '@noble/hashes/utils';
import { decodeJwt } from 'jose';
import { BehaviorSubject, filter, switchMap, takeUntil } from 'rxjs';
import Browser from 'webextension-polyfill';

import NetworkEnv from '../NetworkEnv';
import { Connection } from './Connection';
import { createMessage } from '_messages';
import { type ErrorPayload, isBasePayload } from '_payloads';
import { isSetNetworkPayload, type SetNetworkPayload } from '_payloads/network';
import {
    isGetPermissionRequests,
    isPermissionResponse,
} from '_payloads/permissions';
import { isDisconnectApp } from '_payloads/permissions/DisconnectApp';
import { isGetTransactionRequests } from '_payloads/transactions/ui/GetTransactionRequests';
import { isTransactionRequestResponse } from '_payloads/transactions/ui/TransactionRequestResponse';
import Permissions from '_src/background/Permissions';
import Tabs from '_src/background/Tabs';
import Transactions from '_src/background/Transactions';
import Keyring from '_src/background/keyring';
import { growthbook } from '_src/shared/experimentation/features';

import type { Message } from '_messages';
import type { PortChannelName } from '_messaging/PortChannelName';
import type { LoadedFeaturesPayload } from '_payloads/feature-gating';
import type { KeyringPayload } from '_payloads/keyring';
import type { Permission, PermissionRequests } from '_payloads/permissions';
import type { UpdateActiveOrigin } from '_payloads/tabs/updateActiveOrigin';
import type { ApprovalRequest } from '_payloads/transactions/ApprovalRequest';
import type { GetTransactionRequestsResponse } from '_payloads/transactions/ui/GetTransactionRequestsResponse';
import type { Runtime } from 'webextension-polyfill';

export class UiConnection extends Connection {
    public static readonly CHANNEL: PortChannelName = 'sui_ui<->background';
    private uiAppInitialized: BehaviorSubject<boolean> = new BehaviorSubject(
        false
    );

    constructor(port: Runtime.Port) {
        super(port);
        this.uiAppInitialized
            .pipe(
                filter((init) => init),
                switchMap(() => Tabs.activeOrigin),
                takeUntil(this.onDisconnect)
            )
            .subscribe(({ origin, favIcon }) => {
                this.send(
                    createMessage<UpdateActiveOrigin>({
                        type: 'update-active-origin',
                        origin,
                        favIcon,
                    })
                );
            });
    }

    public async sendLockedStatusUpdate(
        isLocked: boolean,
        replyForId?: string
    ) {
        this.send(
            createMessage<KeyringPayload<'walletStatusUpdate'>>(
                {
                    type: 'keyring',
                    method: 'walletStatusUpdate',
                    return: {
                        isLocked,
                        accounts:
                            (await Keyring.getAccounts())?.map((anAccount) =>
                                anAccount.toJSON()
                            ) || [],
                        activeAddress:
                            (await Keyring.getActiveAccount())?.address || null,
                        isInitialized: await Keyring.isWalletInitialized(),
                    },
                },
                replyForId
            )
        );
    }

    protected async handleMessage(msg: Message) {
        const { payload, id } = msg;
        try {
            if (isGetPermissionRequests(payload)) {
                this.sendPermissions(
                    Object.values(await Permissions.getPermissions()),
                    id
                );
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
                    id
                );
            } else if (isDisconnectApp(payload)) {
                await Permissions.delete(
                    payload.origin,
                    payload.specificAccounts
                );
                this.send(createMessage({ type: 'done' }, id));
            } else if (isBasePayload(payload) && payload.type === 'keyring') {
                await Keyring.handleUiMessage(msg, this);
            } else if (
                isBasePayload(payload) &&
                payload.type === 'get-features'
            ) {
                await growthbook.loadFeatures();
                this.send(
                    createMessage<LoadedFeaturesPayload>(
                        {
                            type: 'features-response',
                            features: growthbook.getFeatures(),
                            attributes: growthbook.getAttributes(),
                        },
                        id
                    )
                );
            } else if (
                isBasePayload(payload) &&
                payload.type === 'get-network'
            ) {
                this.send(
                    createMessage<SetNetworkPayload>(
                        {
                            type: 'set-network',
                            network: await NetworkEnv.getActiveNetwork(),
                        },
                        id
                    )
                );
            } else if (isSetNetworkPayload(payload)) {
                await NetworkEnv.setActiveNetwork(payload.network);
                this.send(createMessage({ type: 'done' }, id));
            } else if (isBasePayload(payload) && payload.type === 'zk-login') {
                const params = new URLSearchParams();
                params.append(
                    'client_id',
                    '946731352276-pk5glcg8cqo38ndb39h7j093fpsphusu.apps.googleusercontent.com'
                );
                params.append('response_type', 'id_token');
                params.append(
                    'redirect_uri',
                    Browser.identity.getRedirectURL()
                );
                params.append('scope', 'openid email');
                params.append('nonce', bytesToHex(randomBytes(16)));
                // This can be used for logins after the user has already connected a google account
                // and we need to make sure that the user logged in with the correct account
                params.append('login_hint', 'test@mystenlabs.com');
                const url = `https://accounts.google.com/o/oauth2/v2/auth?${params.toString()}`;
                const responseURL = new URL(
                    await Browser.identity.launchWebAuthFlow({
                        url,
                        interactive: true,
                    })
                );
                const responseParams = new URLSearchParams(
                    responseURL.hash.replace('#', '')
                );
                const decodedJWT = decodeJwt(
                    responseParams.get('id_token') || ''
                );
                console.log(url, decodedJWT, responseParams);
            }
        } catch (e) {
            console.log(e);
            this.send(
                createMessage<ErrorPayload>(
                    {
                        error: true,
                        code: -1,
                        message: (e as Error).message,
                    },
                    id
                )
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
                requestID
            )
        );
    }

    private sendTransactionRequests(
        txRequests: ApprovalRequest[],
        requestID: string
    ) {
        this.send(
            createMessage<GetTransactionRequestsResponse>(
                {
                    type: 'get-transaction-requests-response',
                    txRequests,
                },
                requestID
            )
        );
    }
}
