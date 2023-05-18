// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ComponentProps } from "react";

export function BackIcon(props: ComponentProps<"svg">) {
  return (
    <svg
      width={24}
      height={24}
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path
        d="M7.57 12.262c0 .341.13.629.403.895l5.175 5.059c.204.205.45.307.751.307.609 0 1.101-.485 1.101-1.087 0-.293-.123-.574-.349-.8L10.14 12.27l4.511-4.375A1.13 1.13 0 0 0 15 7.087C15 6.485 14.508 6 13.9 6c-.295 0-.54.103-.752.308l-5.175 5.058c-.28.28-.404.56-.404.896Z"
        fill="#383F47"
      />
    </svg>
  );
}

export function CloseIcon(props: ComponentProps<"svg">) {
  return (
    <svg
      width={10}
      height={10}
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path
        d="M9.708.292a.999.999 0 0 0-1.413 0l-3.289 3.29L1.717.291A.999.999 0 0 0 .305 1.705l3.289 3.289-3.29 3.289a.999.999 0 1 0 1.413 1.412l3.29-3.289 3.288 3.29a.999.999 0 0 0 1.413-1.413l-3.29-3.29 3.29-3.288a.999.999 0 0 0 0-1.413Z"
        fill="currentColor"
      />
    </svg>
  );
}

export function SuiIcon(props: ComponentProps<"svg">) {
  return (
    <svg
      width={28}
      height={28}
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <rect width={28} height={28} rx={6} fill="#6FBCF0" />
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M7.942 20.527A6.875 6.875 0 0 0 13.957 24c2.51 0 4.759-1.298 6.015-3.473a6.875 6.875 0 0 0 0-6.945l-5.29-9.164a.837.837 0 0 0-1.45 0l-5.29 9.164a6.875 6.875 0 0 0 0 6.945Zm4.524-11.75 1.128-1.953a.418.418 0 0 1 .725 0l4.34 7.516a5.365 5.365 0 0 1 .449 4.442 4.675 4.675 0 0 0-.223-.73c-.599-1.512-1.954-2.68-4.029-3.47-1.426-.54-2.336-1.336-2.706-2.364-.476-1.326.021-2.77.316-3.44Zm-1.923 3.332L9.255 14.34a5.373 5.373 0 0 0 0 5.43 5.373 5.373 0 0 0 4.702 2.714 5.38 5.38 0 0 0 3.472-1.247c.125-.314.51-1.462.034-2.646-.44-1.093-1.5-1.965-3.15-2.594-1.864-.707-3.076-1.811-3.6-3.28a4.601 4.601 0 0 1-.17-.608Z"
        fill="#fff"
      />
    </svg>
  );
}

export function CheckIcon(props: ComponentProps<"svg">) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={16}
      height={16}
      fill="none"
      {...props}
    >
      <path
        fill="#007195"
        d="m11.726 5.048-4.73 5.156-1.722-1.879a.72.72 0 0 0-.529-.23.722.722 0 0 0-.525.24.858.858 0 0 0-.22.573.86.86 0 0 0 .211.576l2.255 2.458c.14.153.332.24.53.24.2 0 .391-.087.532-.24l5.261-5.735A.86.86 0 0 0 13 5.63a.858.858 0 0 0-.22-.572.722.722 0 0 0-.525-.24.72.72 0 0 0-.529.23Z"
      />
    </svg>
  );
}

export function ChevronIcon(props: ComponentProps<"svg">) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={16}
      height={16}
      fill="none"
      {...props}
    >
      <path
        stroke="#A0B6C3"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={1.5}
        d="m4 6 4 4 4-4"
      />
    </svg>
  );
}
