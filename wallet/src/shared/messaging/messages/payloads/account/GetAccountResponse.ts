import type { SuiAddress } from '@mysten/sui.js';
import type { BasePayload } from '_messages/payloads/BasePayload';

export interface GetAccountResponse extends BasePayload {
    type: 'get-account-response';
    accounts: SuiAddress[];
}
