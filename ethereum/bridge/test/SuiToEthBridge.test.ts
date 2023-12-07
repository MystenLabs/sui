// SPDX-License-Identifier: MIT
import { expect } from 'chai'
import { ethers, upgrades } from 'hardhat'
import { loadFixture } from '@nomicfoundation/hardhat-toolbox/network-helpers'
import { ContractTransactionResponse } from 'ethers'
import { ImplementationV2, SuiToEthBridge } from '../typechain-types'

const CONTRACT_NAME = 'SuiToEthBridge'
describe(CONTRACT_NAME, function () {
    let SuiToEthBridge
    let sui2ethBridge: SuiToEthBridge & {
        deploymentTransaction(): ContractTransactionResponse
    }
    let owner
    let validatorAddresses
    let others
    let ImplementationV1
    let implementationV1
    let ImplementationV2
    let implementationV2: ImplementationV2 & {
        deploymentTransaction(): ContractTransactionResponse
    }

    beforeEach(async function () {
        // Get the signers
        ;[owner, ...validatorAddresses] = await ethers.getSigners()
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
})
