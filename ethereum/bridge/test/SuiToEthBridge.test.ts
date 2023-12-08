// SPDX-License-Identifier: UNLICENSED
import { expect } from 'chai'
import { ethers, upgrades } from 'hardhat'
import { loadFixture } from '@nomicfoundation/hardhat-toolbox/network-helpers'
import { ContractTransactionResponse } from 'ethers'
import { ImplementationV1, ImplementationV2, SuiToEthBridge } from '../typechain-types'
import { HardhatEthersSigner } from '@nomicfoundation/hardhat-ethers/signers'

const CONTRACT_NAME = 'SuiToEthBridge'


describe(CONTRACT_NAME, function () {
    let SuiToEthBridge
    let sui2ethBridge: SuiToEthBridge & {deploymentTransaction(): ContractTransactionResponse}
    let owner
    let validatorAddresses: HardhatEthersSigner[]
    let others
    let ImplementationV1
    let implementationV1: ImplementationV1 & { deploymentTransaction(): ContractTransactionResponse }
    let ImplementationV2
    let implementationV2: ImplementationV2 & {deploymentTransaction(): ContractTransactionResponse}

    beforeEach(async function () {
        // Get the signers
        [owner, ...validatorAddresses] = await ethers.getSigners()
        others = validatorAddresses.slice(3)

        // Deploy the mock implementation contracts
        ImplementationV1 = await ethers.getContractFactory('ImplementationV1')
        implementationV1 = await ImplementationV1.deploy()
        // await implementationV1.deployed();
        ImplementationV2 = await ethers.getContractFactory('ImplementationV2')
        implementationV2 = await ImplementationV2.deploy()
        // await implementationV2.deployed();

        // Deploy the SuiToEthBridge contract with the initial implementation and validators
        SuiToEthBridge = await ethers.getContractFactory(CONTRACT_NAME)
        sui2ethBridge = await SuiToEthBridge.deploy(
            implementationV1.getAddress(),
            validatorAddresses
                .slice(0, 3)
                .map((g) => ({ addr: g.getAddress(), weight: 1000 })),
        )
        // await sui2ethBridge.deployed();
    })

    // Test that the SuiToEthBridge contract delegates calls to the implementation contract
    it('should delegate calls to the implementation contract', async function () {
        // Call the getVersion function on the SuiToEthBridge contract
        let version = await sui2ethBridge.getVersion()
        // Expect the version to be 1, as defined in the ImplementationV1 contract
        expect(version).to.equal(1)
    })

      // Test that the SuiToEthBridge contract can be upgraded by the validators
  it("should be upgradable by the validators", async function () {
    // Create an upgrade proposal with the new implementation address and a nonce
    let proposal = {
        newImplementation: await implementationV2.getAddress(),
        nonce: Date.now(),
    };


    let proposalHash = await sui2ethBridge.getProposalHash(proposal);

    // Sign the proposal with the first two guardians
    let signature1 = await validatorAddresses[0].signMessage(ethers.getBytes(proposalHash));
    let signature2 = await validatorAddresses[1].signMessage(ethers.getBytes(proposalHash));


    // Submit the proposal with the first signature
    await sui2ethBridge.connect(validatorAddresses[0]).submitProposal(proposal, signature1);

    // Submit the second signature
    await sui2ethBridge.connect(validatorAddresses[1]).submitSignature(proposal, signature2);

    // Expect the proposal to have two signatures
    expect(await sui2ethBridge.signatures).to.have.lengthOf(2);

    // Execute the upgrade with the proposal and the signatures
    await sui2ethBridge.executeUpgrade(proposal, [signature1, signature2]);

    // Expect the implementation address to be updated
    expect(await sui2ethBridge.implementation()).to.equal(await implementationV2.getAddress());

    // Expect the Upgraded event to be emitted
    expect(sui2ethBridge)
      .to.emit(sui2ethBridge, "Upgraded")
      .withArgs(implementationV1.getAddress(), implementationV2.getAddress());

    // Call the getVersion function on the SuiToEthBridge contract
    let version = await sui2ethBridge.getVersion();
    // Expect the version to be 2, as defined in the ImplementationV2 contract
    expect(version).to.equal(2);
  });
})
