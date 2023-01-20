// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function Stake() {
  return (
    <div className="w-1/2">
      <div className="relative flex items-center">
        <input
          type="text"
          className="block w-full pr-12 bg-white rounded-lg py-2 pl-3 border-steel-darker/30 border"
          placeholder="0 SUI"
        />
        <button className="absolute right-0 flex py-1 px-4 text-sm leading-none bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4] opacity-60 uppercase mr-2 rounded-[4px]">
          Stake
        </button>
      </div>
    </div>
  );
}
