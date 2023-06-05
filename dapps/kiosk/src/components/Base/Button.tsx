// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Spinner } from './Spinner';
import classNames from 'classnames';

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
    <button
      className={classNames(
        'ease-in-out duration-300 rounded border border-transparent py-2 px-4 bg-gray-200',
        className,
      )}
      onClick={onClick}
      {...props}
    >
      {loading ? <Spinner /> : children}
    </button>
  );
}
