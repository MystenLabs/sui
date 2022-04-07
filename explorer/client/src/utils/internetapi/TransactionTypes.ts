
import { AddressBytes, AuthoritySignInfo, GasPayment, Signature, TransactionKind } from "./SuiRpcClient"

type SuiBytes = number[] | string;

type TransactionStatus = string;
type TransactionDigest = SuiBytes;
type ObjectRef = SuiBytes;
type Owner = SuiBytes;
type ObjectRefOwnerPair = [ObjectRef, Owner]

// many types in this file map directly to Rust types of the same name
type TransactionEffects = {
    status: TransactionStatus,
    digest: TransactionDigest,
    created?: ObjectRefOwnerPair[],
    mutated?: ObjectRefOwnerPair[],
    unwrapped?: ObjectRefOwnerPair[],
    deleted?: ObjectRef[],
    wrapped?: ObjectRef[],
    gas?: ObjectRefOwnerPair,
    events?: any[],                     // TODO - better type
    dependencies?: TransactionDigest[]
}

interface TransactionEffectsEnvelope<S> {
    effects: TransactionEffects,
    auth_signature?: S
}

type AuthorityName = string;
type AuthorityNameSigPair = [AuthorityName, SuiBytes];

type TransactionEnvelope<S> = {
    is_checked: boolean,
    data: TransactionData,
    tx_signature: Signature,
    auth_signature?: S
}

type EmptySignInfo = null;
type ClientSignedTransaction = TransactionEnvelope<EmptySignInfo>;

interface CertifiedTransaction {
    digest: SuiBytes,                   // transaction_digest
    checked: boolean,                   // is_checked
    tx: ClientSignedTransaction,        // transaction
    sigs: AuthorityNameSigPair[]        // signatures
}

// this type is NOT in Rust yet - it aggregates the effects and certs types from the Rust backend
interface TransactionDetails<S> {
    data: TransactionEffectsEnvelope<S>,
    certs?: CertifiedTransaction
}

type AuthorityTransactionDetails = TransactionDetails<AuthoritySignInfo>;

interface TransactionData {
    gas_payment: GasPayment;
    kind: TransactionKind;
    sender: AddressBytes;
}

interface Transaction {
    data: TransactionData;
    signature: Signature;
}

// This is the type that should be rendered for a transaction details page
type FriendlyTransactionData = {
    digest: string,
    tx: Transaction,
    effects: TransactionEffects,
    signatures: string[]
}

export type { Transaction, TransactionData, TransactionDetails, AuthorityTransactionDetails, CertifiedTransaction, FriendlyTransactionData }