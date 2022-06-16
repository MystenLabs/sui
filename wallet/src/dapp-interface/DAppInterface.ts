import { filter, fromEvent, lastValueFrom, map, take, tap } from 'rxjs';
import { v4 as uuidV4 } from 'uuid';

import { isErrorPayload } from '_messages/payloads/ErrorPayload';
import { MSG_SENDER_CONTENT, MSG_SENDER_DAPP } from '_messaging/constants';

import type { SuiAddress } from '@mysten/sui.js';
import type { Message } from '_messages/Message';
import type { BasePayload } from '_messages/payloads/BasePayload';
import type { ErrorPayload } from '_messages/payloads/ErrorPayload';
import type { GetAccount } from '_messages/payloads/account/GetAccount';
import type { GetAccountResponse } from '_messages/payloads/account/GetAccountResponse';
import type { Observable } from 'rxjs';

export class DAppInterface {
    private _window: Window;
    private _messagesStream: Observable<Message<any, any>>;

    constructor(theWindow: Window) {
        this._window = window;
        this._messagesStream = fromEvent<MessageEvent<Message<any, any>>>(
            theWindow,
            'message'
        ).pipe(
            tap((e) => console.log('Dapp Received event msg', e.data)),
            filter(
                (e) =>
                    e.source === theWindow &&
                    e.data.sender === MSG_SENDER_CONTENT
            ),
            map((e) => e.data)
        );
    }

    public getAccounts(): Promise<SuiAddress[]> {
        const stream = this.send<GetAccount, GetAccountResponse>({
            type: 'get-account',
        }).pipe(
            take(1),
            map((msg) => {
                if (isErrorPayload(msg.payload)) {
                    // TODO: throw proper error
                    throw new Error(msg.payload.message);
                }
                return msg.payload.accounts;
            })
        );
        return lastValueFrom(stream);
    }

    private send<
        Payload extends BasePayload,
        ResponsePayload extends BasePayload = BasePayload,
        Error = void,
        ResponseError = void
    >(
        payload: Payload | ErrorPayload<Error>,
        responseForID?: string
    ): Observable<Message<ResponsePayload, ResponseError>> {
        const msg: Message<Payload, Error> = {
            id: uuidV4(),
            sender: MSG_SENDER_DAPP,
            responseForID,
            payload,
        };
        this._window.postMessage(msg);
        return this._messagesStream.pipe(
            filter(({ responseForID }) => responseForID === msg.id),
            tap((m) => console.log('Got response message for id', msg.id, m))
        );
    }
}
