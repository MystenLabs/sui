export enum TransactionStatus {
    success = 'success',
    fail = 'fail',
}

export type TransactionType = {
    id: string;
    sender: string;
    created?: string[];
    mutated?: string[];
    deleted?: string[];
    status: TransactionStatus;
};
