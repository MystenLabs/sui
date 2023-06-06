// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactNode } from 'react';
import { Spinner } from './Spinner';
import classNames from 'classnames';

export function Button({
  children,
  loading,
  className,
  onClick,
  ...props
}: {
  children: ReactNode;
  loading?: boolean;
  className?: string;
  onClick: () => Promise<void> | void;
}) {
  return (
    <button
      className={classNames(
        'ease-in-out duration-300 rounded border py-2 px-4 bg-gray-200',
        className,
      )}
      onClick={onClick}
      {...props}
    >
      {loading ? <Spinner /> : children}
    </button>
  );
}
