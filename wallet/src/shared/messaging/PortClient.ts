import { Subject } from 'rxjs';
import Browser from 'webextension-polyfill';

import { PortStream } from './PortStream';

import type { Message } from './messages/Message';
import type { Observable, Subscription } from 'rxjs';

export class PortClient {
    private _channelName: string;
    private _connected: boolean;
    private _portStream: PortStream | null;
    private _msgSub: Subscription | null = null;
    private _disconnectSub: Subscription | null = null;
    private _messageSubject: Subject<Message>;
    private _messageStream: Observable<Message>;
    private _sender: string;

    constructor(channel: string, sender: string) {
        this._channelName = channel;
        this._connected = false;
        this._messageSubject = new Subject();
        this._messageStream = this._messageSubject.asObservable();
        this._portStream = null;
        this._sender = sender;
    }

    public connect() {
        if (!this._connected) {
            this.createPort();
        }
    }

    public get onMessage() {
        return this._messageStream;
    }

    public sendMessage: typeof PortStream['prototype']['sendMessage'] = (
        ...args
    ) => {
        this.connect();
        if (!this._portStream) {
            throw new Error('[PortClient] port stream expected to be defined');
        }
        return this._portStream?.sendMessage(...args);
    };

    private createPort() {
        if (this._connected) {
            return;
        }
        this._connected = true;
        this._portStream = new PortStream(
            Browser.runtime.connect({ name: this._channelName }),
            this._sender
        );
        this._msgSub = this._portStream.onMessage.subscribe((msg) =>
            this._messageSubject.next(msg)
        );
        this._disconnectSub = this._portStream.onDisconnect.subscribe(() => {
            console.log('[PortClient] port disconnected');
            this._connected = false;
            this._msgSub?.unsubscribe();
            this._disconnectSub?.unsubscribe();
            this._portStream = null;
            console.log('[PortClient] auto connecting');
            this.createPort();
        });
    }
}
