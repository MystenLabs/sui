import { expect } from 'chai'
import { ethers, upgrades } from 'hardhat'
import { loadFixture } from '@nomicfoundation/hardhat-toolbox/network-helpers'
import { HardhatEthersSigner } from '@nomicfoundation/hardhat-ethers/signers'
// import { Bridge } from "../typechain-types/contracts/Bridge";
import { Bridge, Bridge__factory } from '../typechain-types'
import { Signer } from 'ethers'

// Define the contract name and the interface
const CONTRACT_NAME = 'Bridge'

// Define an enum for the Message Types
enum MessageType {
    TOKEN,
    COMMITTEE_BLOCKLIST,
    EMERGENCY_OP,
}

// Define an enum for the chain IDs
enum ChainID {
    SUI,
    ETH,
}

// Define an enum for the token IDs
enum TokenID {
    SUI,
    BTC,
    ETH,
    USDC,
    USDT,
}

let contract: Bridge
// let accounts: HardhatEthersSigner[];
let signers: Signer[]
// Initialize the contract before each test
async function beforeEach() {
    // Get the signers from the hardhat provider
    // accounts = await ethers.getSigners();
    signers = randomSigners(100)

    // Deploy the contract using the first account
    const contractFactory = (await ethers.getContractFactory(
        CONTRACT_NAME,
        // accounts[0]
    )) as Bridge__factory
    contract = await contractFactory.deploy()
}

function randomSigners(amount: number) {
    const signers: Signer[] = []
    for (let i = 0; i < amount; i++) {
        signers.push(ethers.Wallet.createRandom())
    }
    return signers
}

// Define the test suite
describe(CONTRACT_NAME, () => {
    // Declare the contract and the accounts variables

    // Test the initialize function
    it('should initialize the contract with the given committee members', async () => {
        await loadFixture(beforeEach)

        // Define the committee members array
        const committeeMembers = await Promise.all(
            signers.map(async (s) => ({
                account: await s.getAddress(),
                stake: 100,
            })),
        )

        // Call the initialize function using the first account
        await contract.initialize(committeeMembers)

        // Check the state variables after initialization
        expect(await contract.validatorsCount()).to.equal(
            committeeMembers.length,
        )
        expect(await contract.running()).to.be.true
        expect(await contract.version()).to.equal(1)
        expect(await contract.messageVersion()).to.equal(1)

        // Check the committee mapping for each member
        for (const member of committeeMembers) {
            const committeeMember = await contract.committee(member.account)
            expect(committeeMember.account).to.equal(member.account)
            expect(committeeMember.stake).to.equal(member.stake)
        }
    })

    // Test the initialize function with a stake above 1000
it("should revert the initialization if the stake is above 1000", async () => {
    await loadFixture(beforeEach)

    // Define the committee members array with one member having a stake of 1100
    const invalidCommitteeMembers = [
      { account: signers[1].getAddress(), stake: 500 },
      { account: signers[2].getAddress(), stake: 300 },
      { account: signers[3].getAddress(), stake: 1100 },
    ];

    // Call the initialize function using the first account and expect it to revert
    await expect(contract.initialize(invalidCommitteeMembers)).to.be
    .revertedWith("Stake is too high");
  });
  
  // Test the initialize function with a total stake above 10000
  it("should revert the initialization if the total stake is above 10000", async () => {
    await loadFixture(beforeEach)

    // Define the committee members array with a total stake of 10001
    const invalidCommitteeMembers = [
        { account: signers[1].getAddress(), stake: 1000 },
        { account: signers[2].getAddress(), stake: 1000 },
        { account: signers[3].getAddress(), stake: 1000 },
        { account: signers[4].getAddress(), stake: 1000 },
        { account: signers[5].getAddress(), stake: 1000 },
        { account: signers[6].getAddress(), stake: 1000 },
        { account: signers[7].getAddress(), stake: 1000 },
        { account: signers[8].getAddress(), stake: 1000 },
        { account: signers[9].getAddress(), stake: 1000 },
        { account: signers[10].getAddress(), stake: 1000 },
        { account: signers[11].getAddress(), stake: 1 },
    ];


    // // Call the initialize function using the first account and expect it to revert
    await expect(contract.initialize(invalidCommitteeMembers)).to.be
    .revertedWith("Total stake is too high");
  });

    /*
    // Test the pauseBridge function
    it('should pause the bridge when called by a committee member', async () => {
        // Initialize the contract with some committee members
        const committeeMembers = [
            { account: accounts[1].address, stake: 500 },
            { account: accounts[2].address, stake: 300 },
            { account: accounts[3].address, stake: 200 },
        ]
        await bridge.connect(accounts[0]).initialize(committeeMembers)

        // Call the pauseBridge function using the second account
        await bridge.connect(accounts[1]).pauseBridge()

        // Check that the running state variable is false
        expect(await bridge.running()).to.be.false
    })

    // Test the resumeBridge function
    it('should resume the bridge when called by a committee member', async () => {
        // Initialize the contract with some committee members
        const committeeMembers = [
            { account: accounts[1].address, stake: 500 },
            { account: accounts[2].address, stake: 300 },
            { account: accounts[3].address, stake: 200 },
        ]
        await bridge.connect(accounts[0]).initialize(committeeMembers)

        // Pause the bridge using the second account
        await bridge.connect(accounts[1]).pauseBridge()

        // Resume the bridge using the third account
        await bridge.connect(accounts[2]).resumeBridge()

        // Check that the running state variable is true
        expect(await bridge.running()).to.be.true
    })

    /*

    // it('should correctly initialize validators', async function () {
    //     const { contract, committee } = await loadFixture(beforeEach)

    //     await contract.initialize(committee)

    //     // // Check if the validators were initialized correctly
    //     for (let i = 0; i < committee.length; i++) {
    //         const committeeMember = await contract.committee(committee[i].account)
    //         expect(committeeMember.account).to.equal(committee[i].account)
    //         expect(committeeMember.stake).to.equal(committee[i].stake)
    //     }

    //     // Check if the validatorsCount matches the expected length
    //     const expectedCount = committee.length
    //     const actualCount = await contract.validatorsCount()
    //     expect(actualCount.toString()).to.equal(expectedCount.toString())
    // })

    it('deploys', async () => {
        const contractFactory = await ethers.getContractFactory(CONTRACT_NAME)
        await contractFactory.deploy()
    })

    // it("Should set the right owner", async function () {
    //   const { contract, owner } = await loadFixture(beforeEach);
    //   expect(await contract.owner()).to.equal(owner.address);
    // });

    /**
        // Write a test case for checking the total weight of validators
        it('should initialize the contract with the first validator and the bridge state', async () => {
            const { contract } = await loadFixture(beforeEach)
    
            // Call the initialize function with the first signer's address and weight
            // await contract.initialize();
    
            // Check if the bridge state is running
            expect(await contract.running()).to.be.false
        })
        */

    // // Test the hashMessage function by comparing the output with the expected hash of a given message
    // it("should return the correct hash of a given message", async () => {
    //   const { contract } = await loadFixture(beforeEach);

    //   // Define a message and its expected hash
    //   const message = "Hello, world!";
    //   const expectedHash =
    //     "0x9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a";

    //   // Call the hashMessage function with the message
    //   const actualHash = await contract.hashMessage(message);

    //   // Compare the actual and expected hashes
    //   expect(actualHash).to.equal(expectedHash);
    // });

    // // Test the verify function by using a valid and an invalid signature for a given message and signer
    // it("should verify a signature for a given message and signer", async () => {
    //   const { contract } = await loadFixture(beforeEach);

    //   // Define a message, a signer, and a valid and an invalid signature
    //   const message = "Hello, world!";
    //   const signer = "0x1234567890123456789012345678901234567890";
    //   const validSignature =
    //     "0x9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a1c";
    //   const invalidSignature =
    //     "0x9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a1d";

    //   // Call the verify function with the message, the signer, and the valid signature
    //   const validResult = await contract.verify(message, validSignature, signer);

    //   // Check if the result is true
    //   expect(validResult).to.be.true;

    //   // Call the verify function with the message, the signer, and the invalid signature
    //   const invalidResult = await contract.verify(
    //     message,
    //     invalidSignature,
    //     signer
    //   );

    //   // Check if the result is false
    //   expect(invalidResult).to.be.false;
    // });

    // // Test the recoverSigner function by using a signature and a message hash that correspond to a known signer
    // it("should recover the signer from a signature and a message hash", async () => {
    //   const { contract } = await loadFixture(beforeEach);

    //   // Define a message hash, a signer, and a signature
    //   const messageHash =
    //     "0x9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a";
    //   const signer = "0x1234567890123456789012345678901234567890";
    //   const signature =
    //     "0x9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a1c";

    //   // Call the recoverSigner function with the message hash and the signature
    //   const recoveredSigner = await contract.recoverSigner(
    //     messageHash,
    //     signature
    //   );

    //   // Compare the recovered signer with the expected signer
    //   expect(recoveredSigner).to.equal(signer);
    // });

    // // Test the recoverSigner function by using a signature and a message hash that correspond to a known signer
    // it("should recover the signer from a signature and a message hash", async () => {
    //   const { contract } = await loadFixture(beforeEach);

    //   // Define a signature and its expected r, s, and v values
    //   const signature =
    //     "0x9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a1c";
    //   const expectedR =
    //     "0x9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a";
    //   const expectedS =
    //     "0x9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a9b4e1a0f7c8f9c0a6c5c2a3d6a0f9f1a";
    //   const expectedV = 28;

    //   // Call the splitSignature function with the signature
    //   const [actualR, actualS, actualV] = await contract.splitSignature(
    //     signature
    //   );

    //   // Compare the actual and expected r, s, and v values
    //   expect(actualR).to.equal(expectedR);
    //   expect(actualS).to.equal(expectedS);
    //   expect(actualV).to.equal(expectedV);
    // });

    /**
    // Write a test case for checking the total weight of validators
    it('should return the correct total weight of validators', async () => {
        const { contract } = await loadFixture(beforeEach)
        // await contract
        //   .initialize
        //   // "0x94926B0ACceE21E61EE900592A039a1075758014",
        //   // 10000
        //   ();

        // Get the expected length of validators from the contract constants
        const expectedWeight = await contract.MAX_TOTAL_WEIGHT()

        // Get the actual length of validators by iterating over the array
        let actualStake = 0
        const arrLength = await contract.validatorsCount()
        for (let i = 0; i < arrLength; i++) {
            // Get the validator at index i
            const committeeMember = await contract.committee(committee[i].account)
            expect(committeeMember.account).to.equal(committee[i].account)
            expect(committeeMember.stake).to.equal(committee[i].stake)

            actualStake += Number(committeeMember.stake)
        }

        // Compare the expected and actual lengths
        expect(actualStake).to.equal(0)

        // expect((await contract.validators).length).to.equal(1);
    })

    // Write a test case for getting the signer from a message hash
    it('should recover the signer from a message', async () => {
        const { contract } = await loadFixture(beforeEach)

        // address, ECDSA.RecoverError, bytes32
        const expectedAddress = '0x5567f54B29B973343d632f7BFCe9507343D41FCa'
        const expectedError = 0n
        const expectedHash =
            '0x0000000000000000000000000000000000000000000000000000000000000000'

        const message = 'Hello, World!'
        const messageHash =
            '0xc21a9f56eed4418969f07d5bb55aecee0f369fdf586f1f6ab8cf5e3b9ec6bf18'
        const signature =
            '0xa4573af531df510a54e86af94f04c368e1705d89de4549e050ed9be02471cdb60c69f4640174c28c0030b8cb93404c8ec420117db37cf753b863c9320ba131d21b'

        // const res = await contract.verifySignature(message, signature);
        const res = await contract.recoverSigner(messageHash, signature)

        expect(res).to.equal(expectedAddress)

        // Compare the expected and actual lengths
        // expect(res[0]).to.equal(expectedAddress);
        // expect(res[1]).to.equal(expectedError);
        // expect(res[2]).to.equal(expectedHash);
    })

    it('should approve the bridge message and return the total weight of valid signatures', async function () {
        const { contract, committee } = await loadFixture(beforeEach)

        expect(await contract.running()).to.be.false

        await contract.initialize(committee)

        expect(await contract.running()).to.be.true

        // Example bridgeMessage
        const bridgeMessage = {
            messageType: MessageType.EMERGENCY_OP,
            messageVersion: 0,
            seqNum: 0,
            sourceChain: ChainID.SUI,
            payload: "0x00",
        }

        // Example signatures array (these would be actual signatures in a real test)
        const signatures = [
            '0x38a816ce06bb5f941789e52d7179137f4c612e7e3430dbabcff26cac780966157138a1ec8ce22e1cdd6176452228cceec86c968c2b604efecefcfb8bb09012f01b',
        ]

        // as [boolean, bigint]
        // const [isValid, totalWeight] = await contract.approveBridgeMessage(
        //   bridgeMessage,
        //   signatures
        // );
        // expect(isValid).to.be.true;
        // expect(totalWeight).to.equal(validators[0].weight);

        await contract.approveBridgeMessage(bridgeMessage, signatures)
        expect(await contract.running()).to.be.false
    })

    it('should resume the bridge if messageType is 1 and total weight is at least 999', async function () {
        const { contract, committee } = await loadFixture(beforeEach)

        expect(await contract.running()).to.be.false

        await contract.initialize(committee)

        expect(await contract.running()).to.be.true

        // Example bridgeMessage
        const bridgeMessage = {
            messageType: MessageType.EMERGENCY_OP,
            messageVersion: 0,
            seqNum: 0,
            sourceChain: ChainID.SUI,
            payload: "0x00"
        }

        // Example signatures array (these would be actual signatures in a real test)
        const signatures = [
            '0x38a816ce06bb5f941789e52d7179137f4c612e7e3430dbabcff26cac780966157138a1ec8ce22e1cdd6176452228cceec86c968c2b604efecefcfb8bb09012f01b',
        ]

        await contract.approveBridgeMessage(bridgeMessage, signatures)
        expect(await contract.running()).to.be.false

        // Call the resumePausedBridge function with the bridgeMessage and signatures
        await contract.resumePausedBridge(bridgeMessage, signatures)

        // Add logic to check if the bridge has been resumed
        expect(await contract.running()).to.be.true
    })

    */

    // it("should pause the bridge", async function () {
    //   const { contract } = await loadFixture(beforeEach);

    //   const validators = [
    //     {
    //       addr: "0x5567f54B29B973343d632f7BFCe9507343D41FCa",
    //       weight: 1000,
    //     },
    //     {
    //       addr: "0x6E78914596C4c3fA605AD25A932564c753353DcC",
    //       weight: 1000,
    //     },
    //   ];

    //   await contract.initialize(validators);

    //   // Example bridgeMessage
    //   const bridgeMessage = {
    //     messageType: 1,
    //     version: 2,
    //     sourceChain: 3,
    //     bridgeSeqNum: 4,
    //     senderAddress: "0x5567f54B29B973343d632f7BFCe9507343D41FCa",
    //     targetChain: 5,
    //     targetAddress: "0x5567f54B29B973343d632f7BFCe9507343D41FCa",
    //   };

    //   // Example signatures array (these would be actual signatures in a real test)
    //   const signatures = [
    //     "0x93f82d7903c6a37336c33d68a890b448665735b4f513003cb4ef0029da0372b9329e0f6fc0b9f9c0c77d66bbf7217da260803fcae345a72f7a7764c56f464b5c1b",
    //   ];

    //   const [isValid, totalWeight] = await contract.approveBridgeMessage(
    //     bridgeMessage,
    //     signatures
    //   );
    //   const paused = await contract.paused();
    //   expect(isValid).to.be.true;
    //   expect(totalWeight).to.equal(validators[0].weight);
    //   expect(paused).to.be.true;
    // });
})
