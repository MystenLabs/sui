// import config before anything else
import { config as dotEnvConfig } from "dotenv";
import "@nomicfoundation/hardhat-ethers";
import "@nomicfoundation/hardhat-toolbox";
import "@openzeppelin/hardhat-upgrades";
import { HardhatUserConfig } from "hardhat/config";

dotEnvConfig();

// read MNEMONIC from file or from env variable
const mnemonic = process.env.MNEMONIC;
// read X-goog-api-key from file or from env variable
const xGoogApiKey = process.env.X_GOOG_API_KEY!;

const config: HardhatUserConfig = {
  solidity: {
    version: "0.8.20",
    settings: {
      optimizer: {
        enabled: true,
        runs: 200,
      },
      viaIR: true,
    },
  },
};

export default config;
