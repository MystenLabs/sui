// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiObjectData } from "@mysten/sui/client";
import { Avatar, Box, Card, Flex, Inset, Text } from "@radix-ui/themes";
import { ReactNode } from "react";
import { ExplorerLink } from "./ExplorerLink";

/**
 * A Card component to view an object's Display (from on-chain data).
 * It includes a label on the top right of the card that can be styled.
 *
 * It also allows for children to be passed in, which will be displayed
 * below the object's display in a footer-like design.
 *
 */
export function SuiObjectDisplay({
  object,
  children,
  label,
  labelClasses,
}: {
  object?: SuiObjectData;
  children?: ReactNode | ReactNode[];
  label?: string;
  labelClasses?: string;
}) {
  const display = object?.display?.data;
  return (
    <Card className="!p-0 sui-object-card">
      {label && (
        <div className={`absolute top-0 right-0 m-2 ${labelClasses}`}>
          {label}
        </div>
      )}
      <Flex gap="3" align="center">
        <Avatar size="6" src={display?.image_url} radius="full" fallback="O" />
        <Box className="grid grid-cols-1">
          <Text className="text-xs">
            <ExplorerLink id={object?.objectId || ""} isAddress={false} />
          </Text>
          <Text as="div" size="2" weight="bold">
            {display?.name || display?.title || "-"}
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
