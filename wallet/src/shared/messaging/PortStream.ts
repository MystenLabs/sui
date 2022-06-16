import { filter, Subject, tap } from 'rxjs';
import { v4 as uuidV4 } from 'uuid';

import type { Message } from './messages/Message';
import type { BasePayload } from './messages/payloads/BasePayload';
import type { ErrorPayload } from './messages/payloads/ErrorPayload';
import type { Observable } from 'rxjs';
import type { Runtime } from 'webextension-polyfill';

export class PortStream {
    private _messagesSubject: Subject<Message>;
    private _messagesStream: Observable<Message>;
    private _disconnectSubject: Subject<Runtime.Port>;
    private _disconnectStream: Observable<Runtime.Port>;
    private _port: Runtime.Port;
    private _sender: string;
    private _connected: boolean;

    constructor(port: Runtime.Port, sender: string) {
        this._messagesSubject = new Subject();
        this._messagesStream = this._messagesSubject.asObservable();
        this._disconnectSubject = new Subject();
        this._disconnectStream = this._disconnectSubject.asObservable();
        this._port = port;
        this._sender = sender;
        this._connected = true;
        this.createConnection();
    }

    public get onMessage(): Observable<Message> {
        return this._messagesStream;
    }

    public get onDisconnect(): Observable<Runtime.Port> {
        return this._disconnectStream;
    }

    public get connected(): boolean {
        return this._connected;
    }

    public sendMessage<T extends BasePayload, E = void>(
        msgPayload: T | ErrorPayload<E>,
        responseForID?: string
    ): Observable<Message> {
        if (!this._port) {
            console.error('[PortStream] port expected to be defined');
            throw new Error('Port to background service worker is not defined');
        }
        const msg: Message<T, E> = {
            id: uuidV4(),
            responseForID,
            sender: this._sender,
            payload: msgPayload,
        };
        this._port.postMessage(msg);
        return this.createResponseObservable(msg.id);
    }

    private createConnection() {
        this._port.onMessage.addListener((msg) => {
            console.log('[PortStream] Received message:', msg);
            this._messagesSubject.next(msg);
        });
        this._port.onDisconnect.addListener(() => {
            console.log('[PortStream] Port disconnected');
            this._disconnectSubject.next(this._port);
            this._disconnectSubject.complete();
            this._messagesSubject.complete();
            this._connected = false;
        });
    }

    private createResponseObservable(msgID: string): Observable<Message> {
        return this._messagesSubject.pipe(
            tap((msg) =>
                console.log(
                    '[Port Stream] received message to filter for response',
                    msg
                )
            ),
            filter((msg) => msg.responseForID === msgID)
        );
    }
}
