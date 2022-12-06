import { Infer, literal, number, object, string, union } from 'superstruct';

export const TransactionDigestStruct = string();
export type TransactionDigest = Infer<typeof TransactionDigestStruct>;

export const ObjectIdStruct = string();
export type ObjectId = Infer<typeof ObjectIdStruct>;

export const SuiAddressStruct = string();
export type SuiAddress = Infer<typeof SuiAddressStruct>;

export const SequenceNumberStruct = number();
export type SequenceNumber = Infer<typeof SequenceNumberStruct>;

export const ObjectOwnerStruct = union([
  object({
    AddressOwner: SuiAddressStruct,
  }),
  object({
    ObjectOwner: SuiAddressStruct,
  }),
  object({
    Shared: object({
      initial_shared_version: number(),
    }),
  }),
  literal('Immutable'),
]);

export type ObjectOwner = Infer<typeof ObjectOwnerStruct>;
