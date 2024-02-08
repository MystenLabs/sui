// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectButton } from "@mysten/dapp-kit";
import { SizeIcon } from "@radix-ui/react-icons";
import { Box, Container, Flex, Heading } from "@radix-ui/themes";
import { Link } from "react-router-dom";

const menu = [
  {
    title: "Manage Objects",
    link: "/locked",
  },
  {
    title: "Escrows",
    link: "/escrows",
  },
];

export function Header() {
  return (
    <Container>
      <Flex
        position="sticky"
        px="4"
        py="2"
        justify="between"
        className="border-b"
      >
        <Box>
          <Heading className="flex items-center gap-3">
            <SizeIcon width={24} height={24} />
            Trading Demo
          </Heading>
        </Box>

        <Box className="flex gap-5 items-center">
          {menu.map((item) => (
            <Link to={item.link} className="cursor-pointer">
              {item.title}
            </Link>
          ))}
        </Box>

        <Box className="connect-wallet-wrapper">
          <ConnectButton />
        </Box>
      </Flex>
    </Container>
  );
}
