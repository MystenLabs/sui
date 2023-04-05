async function main() {
  const [deployer] = await ethers.getSigners();

  console.log("Deploying contracts with the account:", deployer.address);

  const weiAmount = (await deployer.getBalance()).toString();

  console.log("Account balance:", (await ethers.utils.formatEther(weiAmount)));

  const NFT = await ethers.getContractFactory("ERC721Tradable"); // A Move contract
  const nft = await NFT.deploy(
    "Move-on-EVM on a sunny spring day", // name
    "MFT", // symbol
    ethers.constants.AddressZero, // proxyRegistryAddress
    "https://bafybeifwn437bhus4gjmvvsbnwjiv5of7beujekcaavlkdbwnfw7tewi6u.ipfs.nftstorage.link/" // baseURI
  );

  console.log("NFT address:", nft.address);
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
});
