import { create as superstructCreate, Struct } from 'superstruct';

const TupleTag: unique symbol = Symbol();
type TupleTag = typeof TupleTag;

export type WithTupleTag<T, N extends number> = T & { [TupleTag]?: N };

type _TupleOf<T, N extends number, R extends unknown[]> = R['length'] extends N
  ? R
  : _TupleOf<T, N, [T, ...R]>;
export type Tuple<T, N extends number> = N extends N
  ? number extends N
    ? T[]
    : _TupleOf<T, N, []>
  : never;

export function create<T, S>(value: T, struct: Struct<T, S>): T {
  return superstructCreate(value, struct);
}
