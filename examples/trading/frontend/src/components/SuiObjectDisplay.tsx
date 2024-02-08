// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiObjectData } from "@mysten/sui.js/client";
import { Avatar, Box, Card, Flex, Inset, Text } from "@radix-ui/themes";
import { ReactNode } from "react";
import { ObjectLink } from "./ObjectLink";

export function SuiObjectDisplay({
  object,
  children,
}: {
  object?: SuiObjectData;
  children?: ReactNode | ReactNode[];
}) {
  const display = object?.display?.data;
  return (
    <Card className="!p-0 flex flex-col justify-between">
      <Flex gap="3" align="center">
        <Avatar size="6" src={display?.image_url} radius="full" fallback="T" />
        <Box className="grid grid-cols-1">
          <Text className="text-xs">
            <ObjectLink id={object?.objectId || ""} isAddress={false} />
          </Text>
          <Text as="div" size="2" weight="bold">
            {display?.name || display?.title || "No name for this object"}
          </Text>
          <Text as="div" size="2" color="gray">
            {display?.description || "No description for this object."}
          </Text>
        </Box>
      </Flex>
      {children && (
        <Inset className="p-2 border-t mt-3 bg-gray-100 rounded-none">
          {children}
        </Inset>
      )}
    </Card>
  );
}
