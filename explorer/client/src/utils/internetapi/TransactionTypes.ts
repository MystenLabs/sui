
import { AddressBytes, AuthoritySignInfo, GasPayment, Signature, TransactionKind } from "../internetapi/SuiRpcClient"

type SuiBytes = number[] | string;

// types in this file map usually directly to Rust types of the same name
type TransactionEnvelope<S> = {
    is_checked: boolean,
    data: {},       // TransactionData
    tx_signature: Signature,
    auth_signature: Signature
}

type TransactionStatus = string;
type TransactionDigest = SuiBytes;
type ObjectRef = SuiBytes;
type Owner = SuiBytes;
type ObjectRefOwnerPair = [ObjectRef, Owner]

type TransactionEffects = {
    status: TransactionStatus,
    digest: TransactionDigest,
    created?: ObjectRefOwnerPair[],
    mutated?: ObjectRefOwnerPair[],
    unwrapped?: ObjectRefOwnerPair[],
    deleted?: ObjectRef[],
    wrapped?: ObjectRef[],
    gas: ObjectRefOwnerPair,
    events?: any[],                     // TODO - better type
    dependencies: TransactionDigest[]
}

interface TransactionEffectsEnvelope<S> {
    effects: TransactionEffects,
    auth_signature: S
}

type AuthorityName = string;
type AuthorityNameSigPair = [AuthorityName, SuiBytes];

type EmptySignInfo = {};
type ClientSignedTransaction = TransactionEnvelope<EmptySignInfo>;

interface CertifiedTransaction {
    digest: SuiBytes,                   // transaction_digest
    checked: boolean,                   // is_checked
    tx: ClientSignedTransaction,        // transaction
    sigs: AuthorityNameSigPair[]        // signatures
}

// this type is NOT in Rust - it aggregates the effects and certs types from the Rust backend
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

export type { Transaction, TransactionData, TransactionDetails, AuthorityTransactionDetails, CertifiedTransaction }