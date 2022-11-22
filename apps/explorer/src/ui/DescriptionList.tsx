// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ExtractProps } from './types';

import type { ReactNode } from 'react';

import { Text } from '~/ui/Text';





export type LabelProps = ExtractProps<typeof Text>;

export function Label(props: LabelProps) {
  return (
    <dt>
        <Text {...props}>
            {props.children}
        </Text>
    </dt>
  );
}

export function Value({ children }: { children: ReactNode }) {
    return (
      <dd>
          {children}
      </dd>
    );
  }

export type  DescriptionListProps = ExtractProps<typeof Label>;

export function DescriptionList(children : ReactNode) {
    return (
        <dl>
            {children}
        </dl>
    );
}
