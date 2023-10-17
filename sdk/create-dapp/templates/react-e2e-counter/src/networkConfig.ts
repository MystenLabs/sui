import { getFullnodeUrl } from "@mysten/sui.js/client";
import {
  DEVNET_COUNTER_PACKAGE_ID,
  MAINNET_COUNTER_PACKAGE_ID,
} from "./constants.ts";
import { createNetworkConfig } from "@mysten/dapp-kit";

const { networkConfigs, useNetworkConfig } = createNetworkConfig({
  devnet: {
    url: getFullnodeUrl("devnet"),
    counterPackageId: DEVNET_COUNTER_PACKAGE_ID,
  },
  mainnet: {
    url: getFullnodeUrl("mainnet"),
    counterPackageId: MAINNET_COUNTER_PACKAGE_ID,
  },
});

export { useNetworkConfig, networkConfigs };
