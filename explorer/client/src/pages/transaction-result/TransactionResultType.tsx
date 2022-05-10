import type {
    CertifiedTransaction,
    ExecutionStatusType,
    RawObjectRef,
} from '@mysten/sui.js';

export type DataType = CertifiedTransaction & {
    loadState: string;
    txId: string;
    status: ExecutionStatusType;
    gasFee: number;
    txError: string;
    mutated: RawObjectRef[];
    created: RawObjectRef[];
};
