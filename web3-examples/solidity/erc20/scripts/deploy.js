const hre = require("hardhat");

async function main() {
  console.log("Deploying MyToken contract...");

  // Get the contract factory
  const MyToken = await hre.ethers.getContractFactory("MyToken");

  // Deploy the contract
  console.log("Deploying...");
  const myToken = await MyToken.deploy();

  await myToken.waitForDeployment();

  const address = await myToken.getAddress();

  console.log(`✅ MyToken deployed to: ${address}`);

  // Get deployment info
  const [deployer] = await hre.ethers.getSigners();
  const balance = await myToken.balanceOf(deployer.address);
  const totalSupply = await myToken.totalSupply();

  console.log("\nDeployment Info:");
  console.log("================");
  console.log(`Deployer: ${deployer.address}`);
  console.log(`Initial Balance: ${hre.ethers.formatEther(balance)} MTK`);
  console.log(`Total Supply: ${hre.ethers.formatEther(totalSupply)} MTK`);
  console.log(`Max Supply: ${hre.ethers.formatEther(await myToken.MAX_SUPPLY())} MTK`);

  // Save deployment info
  const fs = require("fs");
  const deploymentInfo = {
    network: hre.network.name,
    contract: "MyToken",
    address: address,
    deployer: deployer.address,
    timestamp: new Date().toISOString(),
    totalSupply: totalSupply.toString(),
  };

  fs.writeFileSync(
    `deployments/${hre.network.name}-MyToken.json`,
    JSON.stringify(deploymentInfo, null, 2)
  );

  console.log(`\n💾 Deployment info saved to deployments/${hre.network.name}-MyToken.json`);

  // Verification info
  if (hre.network.name !== "hardhat" && hre.network.name !== "localhost") {
    console.log("\n📝 To verify on Etherscan, run:");
    console.log(`npx hardhat verify --network ${hre.network.name} ${address}`);
  }
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });
