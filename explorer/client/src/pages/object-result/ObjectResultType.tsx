import { type AddressOwner } from '../../utils/internetapi/SuiRpcClient';

export type DataType = {
    id: string;
    category?: string;
    owner: string | AddressOwner;
    version: string;
    readonly?: string;
    objType: string;
    name?: string;
    ethAddress?: string;
    ethTokenId?: string;
    contract_id?: { bytes: string };
    data: {
        contents: {
            [key: string]: any;
        };
        owner?: { ObjectOwner: [] };
        tx_digest?: number[] | string;
    };
    loadState?: string;
};
