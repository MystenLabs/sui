const { BN, constants, expectEvent, expectRevert } = require('@openzeppelin/test-helpers');
const { expect } = require('chai');
const { ZERO_ADDRESS } = constants;

const Revert = artifacts.require('Revert');

contract('Truffle-style testing for Revert (the Move contract)', function (accounts) {
    beforeEach(async function () {
        this.revert = await Revert.new();
    });
    it('revertIf0(0) should revert', async function () {
        await expectRevert.unspecified(this.revert.revertIf0(0));
    });
    it('revertWithMessage() should revert with a message', async function () {
        await expectRevert(this.revert.revertWithMessage(), "error message");
    });
});
