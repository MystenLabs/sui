import { ethers, upgrades } from "hardhat";

async function main() {
  const BridgeNewVersion = await ethers.getContractFactory("Bridge");
  console.log("Upgrading Bridge...");
  const bridgeNewVersion = await upgrades.upgradeProxy("0xf62947A786582d24eA2eb2fb8425525523655b2e", BridgeNewVersion);
  console.log("Bridge upgraded:", await bridgeNewVersion.getAddress());
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
