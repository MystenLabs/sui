import {
  ExecuteTransactionRequestType,
  SuiExecuteTransactionResponseImmediateReturn,
  SuiExecuteTransactionResponseWaitForEffectsCert,
  SuiExecuteTransactionResponseWaitForTxCert,
} from './transactions';

// NOTE: This file cannot be exported from `./types` because it will cause ts-auto-guard to fail.
export type SuiExecuteTransactionResponseTyped<
  RequestType extends ExecuteTransactionRequestType
> = RequestType extends ExecuteTransactionRequestType.ImmediateReturn
  ? SuiExecuteTransactionResponseImmediateReturn
  : RequestType extends ExecuteTransactionRequestType.WaitForTxCert
  ? SuiExecuteTransactionResponseWaitForTxCert
  : RequestType extends ExecuteTransactionRequestType.WaitForEffectsCert
  ? SuiExecuteTransactionResponseWaitForEffectsCert
  : never;
