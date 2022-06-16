import type { BasePayload } from './payloads/BasePayload';
import type { ErrorPayload } from './payloads/ErrorPayload';

export interface Message<Payload extends BasePayload = BasePayload, E = void> {
    id: string;
    responseForID?: string;
    sender: string; // TODO: list of senders not string
    payload: Payload | ErrorPayload<E>;
}
