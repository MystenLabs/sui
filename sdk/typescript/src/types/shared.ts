import {
  array,
  boolean,
  Infer,
  lazy,
  literal,
  number,
  object,
  string,
  union,
  unknown,
} from 'superstruct';

export const TransactionDigest = string();
export type TransactionDigest = Infer<typeof TransactionDigest>;

export const ObjectId = string();
export type ObjectId = Infer<typeof ObjectId>;

export const SuiAddress = string();
export type SuiAddress = Infer<typeof SuiAddress>;

export const SequenceNumber = number();
export type SequenceNumber = Infer<typeof SequenceNumber>;

export const ObjectOwner = union([
  object({
    AddressOwner: SuiAddress,
  }),
  object({
    ObjectOwner: SuiAddress,
  }),
  object({
    Shared: object({
      initial_shared_version: number(),
    }),
  }),
  literal('Immutable'),
]);
export type ObjectOwner = Infer<typeof ObjectOwner>;

// TODO: Figure out if we actually should have validaton on this:
export const SuiJsonValue = unknown();
export type SuiJsonValue = boolean | number | string | Array<SuiJsonValue>;
