type TransactionID = string;
enum TransactionStatus {
    success = 'success',
    fail = 'fail',
}
type AccountAddress = string;
interface Transaction {
    id: TransactionID;
    sender: AccountAddress;
    status: TransactionStatus;
    created: Object[];
    mutated: Object[];
    deleted: Object[];
}
type ObjectID = string;
type Object = {
    id: ObjectID;
} & Record<string, unknown>;
enum SearchItemType {
    transaction = 'tx',
    object = 'obj',
}
interface SearchItem {
    type: SearchItemType;
    item: Transaction | Object;
}
type SearchResponse = SearchItem[];
type TransactionResponse = Transaction;
type TransactionsResponse = Transaction[];

export type SuiApi = {
    'search/{term}': {
        get: {
            200: SearchResponse;
        };
    };
    transactions: {
        get: {
            200: TransactionsResponse;
        };
    };
    'transactions/{id}': {
        get: {
            200: TransactionResponse;
        };
    };
};
