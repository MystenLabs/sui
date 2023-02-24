import {
  array,
  boolean,
  Infer,
  integer,
  object,
  string,
  union,
} from 'superstruct';
import { SharedObjectRef, SuiObjectRef } from '../types';
import { builder } from './bcs';

const ObjectArg = union([
  object({ ImmOrOwned: SuiObjectRef }),
  object({
    Shared: object({
      objectId: string(),
      initialSharedVersion: integer(),
      mutable: boolean(),
    }),
  }),
]);

export const CallArg = union([
  object({ Pure: array(integer()) }),
  object({ Object: ObjectArg }),
]);
export type CallArg = Infer<typeof CallArg>;

export const Inputs = {
  Pure(type: string, data: unknown): CallArg {
    return { Pure: Array.from(builder.ser(type, data).toBytes()) };
  },
  ObjectRef(ref: SuiObjectRef): CallArg {
    return { Object: { ImmOrOwned: ref } };
  },
  SharedObjectRef(ref: SharedObjectRef): CallArg {
    return { Object: { Shared: ref } };
  },
};
