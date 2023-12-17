import { ethers, upgrades } from "hardhat";

async function main() {
  /**
    // Generate 10 private keys
    const privateKeys = Array.from(
        { length: 10 },
        () => ethers.Wallet.createRandom().privateKey,
    )

    // Get the public keys for each private key
    const publicKeys = privateKeys.map((privateKey) =>
        new ethers.Wallet(privateKey).getAddress(),
    )

    console.log('Private keys:', privateKeys)
    console.log('Public keys:', publicKeys)

    const committeeMembers = publicKeys.map((publicKey, index) => {
        return {
            account: publicKey,
            stake: 1000,
        }
    })
    */

    const privateKeys = [
        '0xa0fb551d15a160563f999a6108d10596089928a03959997fd324f64b82851387',
        '0x55e1165ad0a264920c1cc86307abd7ce49e13fd38d68be4551d2afca38bd0ae4',
        '0x11eeebe9cb1f89ddd5dc07c88ef9f9834b9beaaf660f4dbe585e70e97cb6ba4a',
        '0xcf57c5724270cdefc004ece6a782b6699d630b9963c8fb0099968ca646cf999d',
        '0x92a1c1e208ee7f9ae592a94b5ffcde93a2776d87bbd507105c6ca250499e141a',
        '0x0eaa049768ef26747301f0ae8ba6f2e24bce831c81c08c880bda0e6d4efa98b2',
        '0xfce4f6420cfa969aa67b10deafaf080d951b7a6b5068ca73c6676119574e0267',
        '0x2c489f08cd5641eaa14d216cfcce5235d4b63a0bc0e05f08e7cd165910535afd',
        '0xf9841b0983c2cb78dcbab0098c32c70ab914a0d4ebc7605f8b6434fa349ff235',
        '0x41f644343e23d436f227a941398ba7e589c4eb9f8214d92384e91800251e79b0',
    ]
    const publicKeys = [
        '0x2EBDe1Fe7f387c5fF0fD5C43A2C78d59CCf705c4',
        '0x90e55615B26bD34f9b21AbF2D62D3DF48baf9793',
        '0xf7F3764FF720094dF34104af27a0f994Abd7d441',
        '0x2A6b1A7Fa61Cc281f7867cD0f7F6F36b64ebA031',
        '0x5161f030a0271388a9BEE2544Aa77538CA38dAF1',
        '0x89F6664D4D1E39E780bb5c97eca474CfB8766CbB',
        '0x34052dDAAF7a01224Eb1330Aa6C36751a5D5233B',
        '0x8f04B9707df15864201a9abBee3dc036553bFFee',
        '0xD2c77D22735155D4877056972adaaDD1Dc5Dd020',
        '0xb0993760373B5e13B689A7E18DaBa7D960bC9843',
    ]
    const committeeMembers = [
        {
            account: '0x2EBDe1Fe7f387c5fF0fD5C43A2C78d59CCf705c4',
            stake: 1000,
        },
        {
            account: '0x90e55615B26bD34f9b21AbF2D62D3DF48baf9793',
            stake: 1000,
        },
        {
            account: '0xf7F3764FF720094dF34104af27a0f994Abd7d441',
            stake: 1000,
        },
        {
            account: '0x2A6b1A7Fa61Cc281f7867cD0f7F6F36b64ebA031',
            stake: 1000,
        },
        {
            account: '0x5161f030a0271388a9BEE2544Aa77538CA38dAF1',
            stake: 1000,
        },
        {
            account: '0x89F6664D4D1E39E780bb5c97eca474CfB8766CbB',
            stake: 1000,
        },
        {
            account: '0x34052dDAAF7a01224Eb1330Aa6C36751a5D5233B',
            stake: 1000,
        },
        {
            account: '0x8f04B9707df15864201a9abBee3dc036553bFFee',
            stake: 1000,
        },
        {
            account: '0xD2c77D22735155D4877056972adaaDD1Dc5Dd020',
            stake: 1000,
        },
        {
            account: '0xb0993760373B5e13B689A7E18DaBa7D960bC9843',
            stake: 1000,
        },
    ]

    console.log('Committee members:', committeeMembers)

    const Bridge = await ethers.getContractFactory("Bridge");
    console.log("Deploying Bridge...");
    const bridge = await upgrades.deployProxy(Bridge, [committeeMembers], {
      initializer: "initialize",
    });
    await bridge.waitForDeployment();
    console.log("Bridge deployed to:", await bridge.getAddress());
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
main().catch((error) => {
    console.error(error)
    process.exitCode = 1
})
