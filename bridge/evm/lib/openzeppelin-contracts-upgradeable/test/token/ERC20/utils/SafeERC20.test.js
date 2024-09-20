const { constants, expectEvent, expectRevert } = require('@openzeppelin/test-helpers');

const SafeERC20 = artifacts.require('$SafeERC20');
const ERC20ReturnFalseMock = artifacts.require('$ERC20ReturnFalseMock');
const ERC20ReturnTrueMock = artifacts.require('$ERC20'); // default implementation returns true
const ERC20NoReturnMock = artifacts.require('$ERC20NoReturnMock');
const ERC20ForceApproveMock = artifacts.require('$ERC20ForceApproveMock');

const { expectRevertCustomError } = require('../../../helpers/customError');

const name = 'ERC20Mock';
const symbol = 'ERC20Mock';

contract('SafeERC20', function (accounts) {
  const [hasNoCode, receiver, spender] = accounts;

  before(async function () {
    this.mock = await SafeERC20.new();
  });

  describe('with address that has no contract code', function () {
    beforeEach(async function () {
      this.token = { address: hasNoCode };
    });

    it('reverts on transfer', async function () {
      await expectRevertCustomError(this.mock.$safeTransfer(this.token.address, receiver, 0), 'AddressEmptyCode', [
        this.token.address,
      ]);
    });

    it('reverts on transferFrom', async function () {
      await expectRevertCustomError(
        this.mock.$safeTransferFrom(this.token.address, this.mock.address, receiver, 0),
        'AddressEmptyCode',
        [this.token.address],
      );
    });

    it('reverts on increaseAllowance', async function () {
      // Call to 'token.allowance' does not return any data, resulting in a decoding error (revert without reason)
      await expectRevert.unspecified(this.mock.$safeIncreaseAllowance(this.token.address, spender, 0));
    });

    it('reverts on decreaseAllowance', async function () {
      // Call to 'token.allowance' does not return any data, resulting in a decoding error (revert without reason)
      await expectRevert.unspecified(this.mock.$safeDecreaseAllowance(this.token.address, spender, 0));
    });

    it('reverts on forceApprove', async function () {
      await expectRevertCustomError(this.mock.$forceApprove(this.token.address, spender, 0), 'AddressEmptyCode', [
        this.token.address,
      ]);
    });
  });

  describe('with token that returns false on all calls', function () {
    beforeEach(async function () {
      this.token = await ERC20ReturnFalseMock.new(name, symbol);
    });

    it('reverts on transfer', async function () {
      await expectRevertCustomError(
        this.mock.$safeTransfer(this.token.address, receiver, 0),
        'SafeERC20FailedOperation',
        [this.token.address],
      );
    });

    it('reverts on transferFrom', async function () {
      await expectRevertCustomError(
        this.mock.$safeTransferFrom(this.token.address, this.mock.address, receiver, 0),
        'SafeERC20FailedOperation',
        [this.token.address],
      );
    });

    it('reverts on increaseAllowance', async function () {
      await expectRevertCustomError(
        this.mock.$safeIncreaseAllowance(this.token.address, spender, 0),
        'SafeERC20FailedOperation',
        [this.token.address],
      );
    });

    it('reverts on decreaseAllowance', async function () {
      await expectRevertCustomError(
        this.mock.$safeDecreaseAllowance(this.token.address, spender, 0),
        'SafeERC20FailedOperation',
        [this.token.address],
      );
    });

    it('reverts on forceApprove', async function () {
      await expectRevertCustomError(
        this.mock.$forceApprove(this.token.address, spender, 0),
        'SafeERC20FailedOperation',
        [this.token.address],
      );
    });
  });

  describe('with token that returns true on all calls', function () {
    beforeEach(async function () {
      this.token = await ERC20ReturnTrueMock.new(name, symbol);
    });

    shouldOnlyRevertOnErrors(accounts);
  });

  describe('with token that returns no boolean values', function () {
    beforeEach(async function () {
      this.token = await ERC20NoReturnMock.new(name, symbol);
    });

    shouldOnlyRevertOnErrors(accounts);
  });

  describe('with usdt approval beaviour', function () {
    const spender = hasNoCode;

    beforeEach(async function () {
      this.token = await ERC20ForceApproveMock.new(name, symbol);
    });

    describe('with initial approval', function () {
      beforeEach(async function () {
        await this.token.$_approve(this.mock.address, spender, 100);
      });

      it('safeIncreaseAllowance works', async function () {
        await this.mock.$safeIncreaseAllowance(this.token.address, spender, 10);
        expect(this.token.allowance(this.mock.address, spender, 90));
      });

      it('safeDecreaseAllowance works', async function () {
        await this.mock.$safeDecreaseAllowance(this.token.address, spender, 10);
        expect(this.token.allowance(this.mock.address, spender, 110));
      });

      it('forceApprove works', async function () {
        await this.mock.$forceApprove(this.token.address, spender, 200);
        expect(this.token.allowance(this.mock.address, spender, 200));
      });
    });
  });
});

function shouldOnlyRevertOnErrors([owner, receiver, spender]) {
  describe('transfers', function () {
    beforeEach(async function () {
      await this.token.$_mint(owner, 100);
      await this.token.$_mint(this.mock.address, 100);
      await this.token.approve(this.mock.address, constants.MAX_UINT256, { from: owner });
    });

    it("doesn't revert on transfer", async function () {
      const { tx } = await this.mock.$safeTransfer(this.token.address, receiver, 10);
      await expectEvent.inTransaction(tx, this.token, 'Transfer', {
        from: this.mock.address,
        to: receiver,
        value: '10',
      });
    });

    it("doesn't revert on transferFrom", async function () {
      const { tx } = await this.mock.$safeTransferFrom(this.token.address, owner, receiver, 10);
      await expectEvent.inTransaction(tx, this.token, 'Transfer', {
        from: owner,
        to: receiver,
        value: '10',
      });
    });
  });

  describe('approvals', function () {
    context('with zero allowance', function () {
      beforeEach(async function () {
        await this.token.$_approve(this.mock.address, spender, 0);
      });

      it("doesn't revert when force approving a non-zero allowance", async function () {
        await this.mock.$forceApprove(this.token.address, spender, 100);
        expect(await this.token.allowance(this.mock.address, spender)).to.be.bignumber.equal('100');
      });

      it("doesn't revert when force approving a zero allowance", async function () {
        await this.mock.$forceApprove(this.token.address, spender, 0);
        expect(await this.token.allowance(this.mock.address, spender)).to.be.bignumber.equal('0');
      });

      it("doesn't revert when increasing the allowance", async function () {
        await this.mock.$safeIncreaseAllowance(this.token.address, spender, 10);
        expect(await this.token.allowance(this.mock.address, spender)).to.be.bignumber.equal('10');
      });

      it('reverts when decreasing the allowance', async function () {
        await expectRevertCustomError(
          this.mock.$safeDecreaseAllowance(this.token.address, spender, 10),
          'SafeERC20FailedDecreaseAllowance',
          [spender, 0, 10],
        );
      });
    });

    context('with non-zero allowance', function () {
      beforeEach(async function () {
        await this.token.$_approve(this.mock.address, spender, 100);
      });

      it("doesn't revert when force approving a non-zero allowance", async function () {
        await this.mock.$forceApprove(this.token.address, spender, 20);
        expect(await this.token.allowance(this.mock.address, spender)).to.be.bignumber.equal('20');
      });

      it("doesn't revert when force approving a zero allowance", async function () {
        await this.mock.$forceApprove(this.token.address, spender, 0);
        expect(await this.token.allowance(this.mock.address, spender)).to.be.bignumber.equal('0');
      });

      it("doesn't revert when increasing the allowance", async function () {
        await this.mock.$safeIncreaseAllowance(this.token.address, spender, 10);
        expect(await this.token.allowance(this.mock.address, spender)).to.be.bignumber.equal('110');
      });

      it("doesn't revert when decreasing the allowance to a positive value", async function () {
        await this.mock.$safeDecreaseAllowance(this.token.address, spender, 50);
        expect(await this.token.allowance(this.mock.address, spender)).to.be.bignumber.equal('50');
      });

      it('reverts when decreasing the allowance to a negative value', async function () {
        await expectRevertCustomError(
          this.mock.$safeDecreaseAllowance(this.token.address, spender, 200),
          'SafeERC20FailedDecreaseAllowance',
          [spender, 100, 200],
        );
      });
    });
  });
}
