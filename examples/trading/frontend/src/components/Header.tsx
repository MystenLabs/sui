// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGenerateDemoData } from "@/mutations/demo";
import { ConnectButton } from "@mysten/dapp-kit";
import { SizeIcon } from "@radix-ui/react-icons";
import { Box, Button, Container, Flex, Heading } from "@radix-ui/themes";
import { NavLink } from "react-router-dom";

const menu = [
  {
    title: "Escrows",
    link: "/escrows",
  },
  {
    title: "Manage Objects",
    link: "/locked",
  },
];

export function Header() {
  const { mutate: demoBearMutation, isPending } = useGenerateDemoData();
  return (
    <Container>
      <Flex
        position="sticky"
        px="4"
        py="2"
        justify="between"
        className="border-b flex flex-wrap"
      >
        <Box>
          <Heading className="flex items-center gap-3">
            <SizeIcon width={24} height={24} />
            Trading Demo
          </Heading>
        </Box>

        <Box className="flex gap-5 items-center">
          {menu.map((item) => (
            <NavLink
              key={item.link}
              to={item.link}
              className={({ isActive, isPending }) =>
                `cursor-pointer flex items-center gap-2 ${
                  isPending
                    ? "pending"
                    : isActive
                      ? "font-bold text-blue-600"
                      : ""
                }`
              }
            >
              {item.title}
            </NavLink>
          ))}
        </Box>
        <Box>
          <Button
            className="cursor-pointer"
            disabled={isPending}
            onClick={() => {
              demoBearMutation();
            }}
          >
            New Demo Bear
          </Button>
        </Box>

        <Box className="connect-wallet-wrapper">
          <ConnectButton />
        </Box>
      </Flex>
    </Container>
  );
}
