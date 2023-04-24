// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from "react";
const SvgSui = (props: SVGProps<SVGSVGElement>) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="1em"
    height="1em"
    fill="none"
    viewBox="0 0 16 16"
    {...props}
  >
    <path
      fill="currentColor"
      fillRule="evenodd"
      d="M3.677 12.628A4.941 4.941 0 0 0 8 15.124c1.805 0 3.42-.933 4.323-2.496a4.94 4.94 0 0 0 0-4.991L8.521 1.05a.601.601 0 0 0-1.042 0L3.677 7.637a4.941 4.941 0 0 0 0 4.991Zm3.252-8.444.81-1.404a.3.3 0 0 1 .521 0l3.12 5.402c.572.992.68 2.14.322 3.192a3.353 3.353 0 0 0-.16-.524c-.43-1.087-1.405-1.926-2.895-2.494-1.025-.389-1.68-.96-1.945-1.7-.343-.952.015-1.991.227-2.472ZM5.546 6.578l-.925 1.604a3.862 3.862 0 0 0 0 3.902A3.862 3.862 0 0 0 8 14.034c.937 0 1.81-.321 2.496-.896.09-.225.367-1.05.024-1.901-.316-.786-1.078-1.413-2.264-1.864-1.34-.509-2.21-1.302-2.587-2.359a3.31 3.31 0 0 1-.123-.436Z"
      clipRule="evenodd"
    />
  </svg>
);
export default SvgSui;
