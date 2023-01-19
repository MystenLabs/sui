// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Round number.
 *
 * Requires reading the SuiSystem object to get current epoch
 * minus the start round for the Frenemies game.
 */
export function Round({ num }: { num: number }) {
  return (
    <h2 className="uppercase text-steel-dark font-thin text-6xl sm:text-8xl md:text-9xl lg:text-9xl xl:text-[160px] leading-tight text-center tracking-widest">
      Round {num}
    </h2>
  );
}
