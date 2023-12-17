import { expect } from 'chai'
import { ethers, upgrades } from 'hardhat'
import { loadFixture } from '@nomicfoundation/hardhat-toolbox/network-helpers'
import { HardhatEthersSigner } from '@nomicfoundation/hardhat-ethers/signers'
// import { Bridge } from "../typechain-types/contracts/Bridge";
import { Bridge, Bridge__factory } from '../typechain-types'
import { Signer, parseEther } from 'ethers'

// Define the contract name and the interface
const CONTRACT_NAME = 'Bridge'

enum ChainID {
    SUI_MAINNET,
    SUI_TESTNET,
    SUI_DEVNET,
    ETH_MAINNET,
    ETH_SEPOLIA,
}

enum EmergencyOpType {
    FREEZE,
    UNFREEZE,
}

enum MessageType {
    TOKEN,
    COMMITTEE_BLOCKLIST,
    EMERGENCY_OP,
}

enum TokenID {
    SUI,
    BTC,
    ETH,
    USDC,
    USDT,
}

enum BlockListType {
    BLOCKLIST,
    UNBLOCKLIST,
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
    for (let i = 0; i < amount; i++) signers.push(ethers.Wallet.createRandom())
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
    it('should revert the initialization if the stake is above 1000', async () => {
        await loadFixture(beforeEach)

        // Define the committee members array with one member having a stake of 1100
        const invalidCommitteeMembers = [
            { account: signers[1].getAddress(), stake: 500 },
            { account: signers[2].getAddress(), stake: 300 },
            { account: signers[3].getAddress(), stake: 1100 },
        ]

        // Call the initialize function using the first account and expect it to revert
        await expect(
            contract.initialize(invalidCommitteeMembers),
        ).to.be.revertedWith('Stake is too high')
    })

    // Test the initialize function with a total stake above 10000
    it('should revert the initialization if the total stake is above 10000', async () => {
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
        ]

        // // Call the initialize function using the first account and expect it to revert
        await expect(
            contract.initialize(invalidCommitteeMembers),
        ).to.be.revertedWith('Total stake is too high')
    })

    it('should pause the bridge and increment the sequence number', async () => {
        await loadFixture(beforeEach)

        // Create a mock bridge message
        const bridgeMessage = {
            messageType: MessageType.EMERGENCY_OP, // 2
            messageVersion: 1,
            sequenceNumber: 0,
            sourceChain: ChainID.SUI_MAINNET, // 0
            payload: '0x00',
        }

        // Define the committee members array
        let committeeMembers: { account: string; stake: number }[] = []
        let signatures: string[] = []

        for (let i = 0; i < 10; i++) {
            let wallet = ethers.Wallet.createRandom()
            committeeMembers.push({
                account: await wallet.getAddress(),
                stake: 1000,
            })

            const messageHash = ethers.solidityPackedKeccak256(
                ['uint8', 'uint8', 'uint64', 'uint8', 'bytes'],
                [
                    bridgeMessage.messageType,
                    bridgeMessage.messageVersion,
                    bridgeMessage.sequenceNumber,
                    bridgeMessage.sourceChain,
                    bridgeMessage.payload,
                ],
            )
            const messageHashBinary = ethers.getBytes(messageHash)
            const signature = await wallet.signMessage(messageHashBinary)
            signatures.push(signature)
        }

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

        // Freeze the bridge
        await contract.freezeBridge(bridgeMessage, signatures)
        expect(await contract.running()).to.be.false
        expect(
            await contract.sequenceNumbers(bridgeMessage.messageType),
        ).to.equal(0)
    })

    it('should pause and unpause the bridge and increment the sequence number', async () => {
        await loadFixture(beforeEach)

        // Create a mock bridge message
        const bridgeMessage = {
            messageType: MessageType.EMERGENCY_OP, // 2
            messageVersion: 1,
            sequenceNumber: 0,
            sourceChain: ChainID.SUI_MAINNET, // 0
            payload: '0x00',
        }

        // Define the committee members array
        let committeeMembers: { account: string; stake: number }[] = []
        let signatures: string[] = []

        for (let i = 0; i < 10; i++) {
            let wallet = ethers.Wallet.createRandom()
            committeeMembers.push({
                account: await wallet.getAddress(),
                stake: 1000,
            })

            const messageHash = ethers.solidityPackedKeccak256(
                ['uint8', 'uint8', 'uint64', 'uint8', 'bytes'],
                [
                    bridgeMessage.messageType,
                    bridgeMessage.messageVersion,
                    bridgeMessage.sequenceNumber,
                    bridgeMessage.sourceChain,
                    bridgeMessage.payload,
                ],
            )
            const messageHashBinary = ethers.getBytes(messageHash)
            const signature = await wallet.signMessage(messageHashBinary)
            signatures.push(signature)
        }

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

        // Freeze the bridge
        await contract.freezeBridge(bridgeMessage, signatures)
        expect(await contract.running()).to.be.false
        expect(
            await contract.sequenceNumbers(bridgeMessage.messageType),
        ).to.equal(0)

        // Unfreeze the bridge
        await contract.unfreezeBridge(bridgeMessage, signatures)
        expect(await contract.running()).to.be.true
        expect(
            await contract.sequenceNumbers(bridgeMessage.messageType),
        ).to.equal(1)
    })

    it('should revert the pause of the bridge and increment the sequence number', async () => {
        await loadFixture(beforeEach)

        // Create a mock bridge message
        const bridgeMessage = {
            messageType: MessageType.EMERGENCY_OP, // 2
            messageVersion: 1,
            sequenceNumber: 0,
            sourceChain: ChainID.SUI_MAINNET, // 0
            payload: '0x00',
        }

        // Define the committee members array
        let committeeMembers: { account: string; stake: number }[] = []
        let signatures: string[] = []

        for (let i = 0; i < 10; i++) {
            let wallet = ethers.Wallet.createRandom()
            committeeMembers.push({
                account: await wallet.getAddress(),
                stake: 1000,
            })

            const messageHash = ethers.solidityPackedKeccak256(
                ['uint8', 'uint8', 'uint64', 'uint8', 'bytes'],
                [
                    bridgeMessage.messageType,
                    bridgeMessage.messageVersion,
                    bridgeMessage.sequenceNumber,
                    bridgeMessage.sourceChain,
                    bridgeMessage.payload,
                ],
            )
            const messageHashBinary = ethers.getBytes(messageHash)
            const signature = await wallet.signMessage(messageHashBinary)
            signatures.push(signature)
        }

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

        // Call the freezeBridge function using only 1 signature and expect it to revert
        await expect(
            contract.freezeBridge(bridgeMessage, [signatures[0]]),
        ).to.be.revertedWith(
            'Not enough signatures to approve the emergency operation',
        )
        expect(await contract.running()).to.be.true
        expect(
            await contract.sequenceNumbers(bridgeMessage.messageType),
        ).to.equal(0)
    })

    it('should pause and unpause the bridge and increment the sequence number', async () => {
        await loadFixture(beforeEach)

        // Create a mock bridge message
        const bridgeMessage = {
            messageType: MessageType.EMERGENCY_OP, // 2
            messageVersion: 1,
            sequenceNumber: 0,
            sourceChain: ChainID.SUI_MAINNET, // 0
            payload: '0x00',
        }

        // Define the committee members array
        let committeeMembers: { account: string; stake: number }[] = []
        let signatures: string[] = []

        for (let i = 0; i < 10; i++) {
            let wallet = ethers.Wallet.createRandom()
            committeeMembers.push({
                account: await wallet.getAddress(),
                stake: 1000,
            })

            const messageHash = ethers.solidityPackedKeccak256(
                ['uint8', 'uint8', 'uint64', 'uint8', 'bytes'],
                [
                    bridgeMessage.messageType,
                    bridgeMessage.messageVersion,
                    bridgeMessage.sequenceNumber,
                    bridgeMessage.sourceChain,
                    bridgeMessage.payload,
                ],
            )
            const messageHashBinary = ethers.getBytes(messageHash)
            const signature = await wallet.signMessage(messageHashBinary)
            signatures.push(signature)
        }

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

        // Freeze the bridge
        await contract.freezeBridge(bridgeMessage, signatures)
        expect(await contract.running()).to.be.false
        expect(
            await contract.sequenceNumbers(bridgeMessage.messageType),
        ).to.equal(0)

        // Call the unfreezeBridge function using only 5 signatures (50% stake) and expect it to revert
        await expect(
            contract.unfreezeBridge(bridgeMessage, [signatures[0]]),
        ).to.be.revertedWith(
            'Not enough signatures to approve the emergency operation',
        )
        await contract.unfreezeBridge(bridgeMessage, signatures)
        expect(await contract.running()).to.be.true
        expect(
            await contract.sequenceNumbers(bridgeMessage.messageType),
        ).to.equal(1)
    })

    it('should emit a BridgeEvent with the correct values', async () => {
        await loadFixture(beforeEach)

        // Create a mock bridge message
        const bridgeMessage = {
            messageType: MessageType.EMERGENCY_OP, // 2
            messageVersion: 1,
            sequenceNumber: 0,
            sourceChain: ChainID.SUI_MAINNET, // 0
            payload: '0x00',
        }

        // Define the committee members array
        let committeeMembers: { account: string; stake: number }[] = []
        let signatures: string[] = []

        for (let i = 0; i < 10; i++) {
            let wallet = ethers.Wallet.createRandom()
            committeeMembers.push({
                account: await wallet.getAddress(),
                stake: 1000,
            })

            const messageHash = ethers.solidityPackedKeccak256(
                ['uint8', 'uint8', 'uint64', 'uint8', 'bytes'],
                [
                    bridgeMessage.messageType,
                    bridgeMessage.messageVersion,
                    bridgeMessage.sequenceNumber,
                    bridgeMessage.sourceChain,
                    bridgeMessage.payload,
                ],
            )
            const messageHashBinary = ethers.getBytes(messageHash)
            const signature = await wallet.signMessage(messageHashBinary)
            signatures.push(signature)
        }

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

        const [msgSender] = await ethers.getSigners()
        await contract.initBridgingTokenTx(await msgSender.getAddress(), {
            value: parseEther('1.0'),
        })

        expect(await contract.getBalance()).to.equal(parseEther('1.0'))
    })

    it('should verify token bridging signatures', async () => {
        await loadFixture(beforeEach)

        // Create a mock bridge message
        const tbm = {
            messageType: MessageType.TOKEN,
            messageVersion: 1,
            nonce: 0,
            sourceChain: ChainID.ETH_MAINNET,
            sourceChainTxIdLength: 0,
            sourceChainTxId: '0x00',
            sourceChainEventIndex: 0,
            senderAddressLength: 0,
            senderAddress: '0x00',
            targetChain: ChainID.SUI_MAINNET,
            targetChainLength: 0,
            targetAddress: '0x00',
            tokenType: TokenID.ETH,
            amount: 1000,
        }

        // Define the committee members array
        let committeeMembers: { account: string; stake: number }[] = []
        let signatures: string[] = []

        for (let i = 0; i < 10; i++) {
            let wallet = ethers.Wallet.createRandom()
            committeeMembers.push({
                account: await wallet.getAddress(),
                stake: 1000,
            })

            const messageHash = ethers.solidityPackedKeccak256(
                [
                    'string',
                    'uint8',
                    'uint8',
                    'uint64',
                    'uint8',
                    'uint8',
                    'bytes',
                    'uint8',
                    'uint8',
                    'bytes',
                    'uint8',
                    'uint8',
                    'bytes',
                    'uint8',
                    'uint64',
                ],
                [
                    'SUI_NATIVE_BRIDGE',
                    tbm.messageType,
                    tbm.messageVersion,
                    tbm.nonce,
                    tbm.sourceChain,
                    tbm.sourceChainTxIdLength,
                    tbm.sourceChainTxId,
                    tbm.sourceChainEventIndex,
                    tbm.senderAddressLength,
                    tbm.senderAddress,
                    tbm.targetChain,
                    tbm.targetChainLength,
                    tbm.targetAddress,
                    tbm.tokenType,
                    tbm.amount,
                ],
            )
            const messageHashBinary = ethers.getBytes(messageHash)
            const signature = await wallet.signMessage(messageHashBinary)
            signatures.push(signature)
        }

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

        // verifyTokenBridgingSignatures
        const [seen, totalStake] = await contract.verifyTokenBridgingSignatures(
            tbm,
            signatures,
        )
        expect(seen.length).to.equal(committeeMembers.length)
        expect(totalStake).to.equal(
            committeeMembers.map((c) => c.stake).reduce((a, b) => a + b, 0),
        )
    })
})
