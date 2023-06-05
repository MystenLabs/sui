// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Spinner } from './Spinner';

export function Button({
  children,
  loading,
  className,
  onClick,
  ...props
}: {
  children: JSX.Element[] | JSX.Element | string;
  loading?: boolean;
  className?: string;
  onClick: () => Promise<void> | void;
}): JSX.Element {
  return (
    <button className={className} onClick={onClick} {...props}>
      {loading ? <Spinner /> : children}
    </button>
  );
}
