import { create as superstructCreate, Struct } from 'superstruct';

export function create<T, S>(value: T, struct: Struct<T, S>): T {
  return superstructCreate(value, struct);
}
