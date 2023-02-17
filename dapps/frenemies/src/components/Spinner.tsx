// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function Spinner() {
  return (
    <div className="text-steel-darker mt-32 mx-auto">
      <svg
        width={16}
        height={16}
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className="animate-spin text-steel-darker"
      >
        <path
          d="M2.204 6.447A6 6 0 1 0 8 2"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
        />
      </svg>
    </div>
  );
}
