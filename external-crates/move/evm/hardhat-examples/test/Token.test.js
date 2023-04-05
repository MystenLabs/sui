const { expect } = require("chai");

describe("Token (the Move contract)", function () {
    before(async function () {
        this.Token = await ethers.getContractFactory("Token");
        this.token = await this.Token.deploy("user");
        await this.token.deployed();
    });

    it("test total supply", async function () {
        const accounts = await ethers.getSigners();
        deployer = accounts[0];

        expect(await this.token.balanceOf(deployer.address)).to.be.equal(42);
        expect(await this.token.totalSupply()).to.be.equal(42);
    });

    it("test mint", async function () {
        const accounts = await ethers.getSigners();
        alice = accounts[1];

        const mintTx = await this.token.mint(alice.address, 127);
        // wait until the transaction is mined
        await mintTx.wait();
        expect(await this.token.balanceOf(alice.address)).to.be.equal(127);
        expect(await this.token.totalSupply()).to.be.equal(169);
    });

    it("test transfer", async function () {
        const accounts = await ethers.getSigners();
        deployer = accounts[0];
        bob = accounts[2];

        // Transfer 17 tokens from the deployer to alice
        const transferTx = await this.token.transfer(bob.address, 17);
        // wait until the transaction is mined
        await transferTx.wait();

        expect(await this.token.balanceOf(bob.address)).to.be.equal(17);
        expect(await this.token.balanceOf(deployer.address)).to.be.equal(25);
        // total supply should not change
        expect(await this.token.totalSupply()).to.be.equal(169);
    });

    it("test name", async function () {
        const accounts = await ethers.getSigners();
        deployer = accounts[0];
        expect(await this.token.name()).to.be.equal("user");
    });
});
