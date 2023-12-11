import { expect } from "chai";
import { ethers, upgrades } from "hardhat";
import { loadFixture } from "@nomicfoundation/hardhat-toolbox/network-helpers";
import { HardhatEthersSigner } from '@nomicfoundation/hardhat-ethers/signers'

// Define the contract name and the interface
const CONTRACT_NAME = "Bridge";
const CONTRACT_INTERFACE = [
  "function initialize() public",
  "function hashMessage(string) public pure returns (bytes32)",
  "function uintToStr(uint) internal pure returns (string)",
  "function messageHash(string) public pure returns (bytes32)",
  "function ethereumthSignedMessageHash(bytes32) public pure returns (bytes32)",
  "function verify(string, bytes, address) public pure returns (bool)",
  "function recoverSigner(bytes32, bytes) public pure returns (address)",
  "function splitSignature(bytes) public pure returns (bytes32, bytes32, uint8)",
  "function strlen(string) private pure returns (uint256)",
  "function contains(address[], address) private pure returns (bool)",
  "function addValidator(address, uint256) private",
  "function validatorsCount() public view returns (uint)",
  "function verifyFunction(string memory message, bytes memory signature) external pure returns (address, ECDSA.RecoverError, bytes32)",
  "function approveBridgeMessage(BridgeMessage calldata bridgeMessage, bytes[] calldata signatures) public isRunning returns (bool, uint256)",
];

	// Define an enum for the Message Types
	enum MessageType {
        TOKEN,
        COMMITTEE_BLOCKLIST,
        EMERGENCY_OP
	}

	// Define an enum for the chain IDs
	enum ChainID {
        SUI,
        ETH
	}

	// Define an enum for the token IDs
	enum TokenID {
        SUI,
        BTC,
        ETH,
        USDC,
        USDT
	}

// Write a test suite for the contract
describe(CONTRACT_NAME, () => {
    let committee: { account: string; stake: number }[]
    let hardhatEthersSigners: HardhatEthersSigner[]
    let others

    // Deploy the contract before each test
    async function beforeEach() {
        // Get the signers from the hardhat network
        let [owner, ...hardhatEthersSigners] = await ethers.getSigners()
        others = hardhatEthersSigners.slice(2)

        // Get the contract factory and deploy the contract
        const contractFactory = await ethers.getContractFactory(CONTRACT_NAME)
        const contract = await contractFactory.deploy()

        const tmpCommittee = await Promise.all(
            hardhatEthersSigners
                .slice(1)
                .map(async (g) => ({
                    account: await g.getAddress(),
                    stake: 1000,
                })),
        )

        // Create the new validator object
        const defaultCommitteeMember = {
            account: '0x5567f54B29B973343d632f7BFCe9507343D41FCa',
            stake: 1000,
        }

        committee = [defaultCommitteeMember, ...tmpCommittee]

        return { contract, owner, committee }
    }

    it('should correctly initialize validators', async function () {
        const { contract, committee } = await loadFixture(beforeEach)

        await contract.initialize(committee)

        // // Check if the validators were initialized correctly
        for (let i = 0; i < committee.length; i++) {
            const committeeMember = await contract.committee(committee[i].account)
            expect(committeeMember.account).to.equal(committee[i].account)
            expect(committeeMember.stake).to.equal(committee[i].stake)
        }

        // Check if the validatorsCount matches the expected length
        const expectedCount = committee.length
        const actualCount = await contract.validatorsCount()
        expect(actualCount.toString()).to.equal(expectedCount.toString())
    })

    it('deploys', async () => {
        const contractFactory = await ethers.getContractFactory(CONTRACT_NAME)
        await contractFactory.deploy()
    })

    // it("Should set the right owner", async function () {
    //   const { contract, owner } = await loadFixture(beforeEach);
    //   expect(await contract.owner()).to.equal(owner.address);
    // });

    // Write a test case for checking the total weight of validators
    it('should initialize the contract with the first validator and the bridge state', async () => {
        const { contract } = await loadFixture(beforeEach)

        // Call the initialize function with the first signer's address and weight
        // await contract.initialize();

        // Check if the bridge state is running
        expect(await contract.running()).to.be.false
    })

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
