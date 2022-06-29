// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { filter, lastValueFrom, map, take } from 'rxjs';

import { createMessage } from '_messages';
import { WindowMessageStream } from '_messaging/WindowMessageStream';
import { isErrorPayload } from '_payloads';

import type { SuiAddress } from '@mysten/sui.js';
import type { Payload } from '_payloads';
import type { GetAccount } from '_payloads/account/GetAccount';
import type { GetAccountResponse } from '_payloads/account/GetAccountResponse';
import type { Observable } from 'rxjs';

export class DAppInterface {
    private _messagesStream: WindowMessageStream;

    constructor() {
        this._messagesStream = new WindowMessageStream(
            'sui_in-page',
            'sui_content-script'
        );
    }

    public getAccounts(): Promise<SuiAddress[]> {
        const stream = this.send<GetAccount, GetAccountResponse>({
            type: 'get-account',
        }).pipe(
            take(1),
            map((response) => {
                if (isErrorPayload(response)) {
                    // TODO: throw proper error
                    throw new Error(response.message);
                }
                return response.accounts;
            })
        );
        return lastValueFrom(stream);
    }

    private send<
        RequestPayload extends Payload,
        ResponsePayload extends Payload | void = void
    >(
        payload: RequestPayload,
        responseForID?: string
    ): Observable<ResponsePayload> {
        const msg = createMessage(payload, responseForID);
        this._messagesStream.send(msg);
        return this._messagesStream.messages.pipe(
            filter(({ id }) => id === msg.id),
            map((msg) => msg.payload as ResponsePayload)
        );
    }
}
