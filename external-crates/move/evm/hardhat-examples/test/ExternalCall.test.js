const { BN, constants, expectEvent, expectRevert } = require('@openzeppelin/test-helpers');
const { ZERO_ADDRESS } = constants;

const { expect } = require('chai');

const ExternalCall = artifacts.require('ExternalCall');
const FortyTwo = artifacts.require('FortyTwo');
const Revert = artifacts.require('Revert');
const ERC721Receiver = artifacts.require('ERC721ReceiverMock');
const ERC1155Receiver = artifacts.require('ERC1155ReceiverMock');

const Error = [ 'None', 'RevertWithMessage', 'RevertWithoutMessage', 'Panic' ]
  .reduce((acc, entry, idx) => Object.assign({ [entry]: idx }, acc), {});
const RECEIVER_MAGIC_VALUE = '0x150b7a02';
const RECEIVER_SINGLE_MAGIC_VALUE = '0xf23a6e61';
const RECEIVER_BATCH_MAGIC_VALUE = '0xbc197c81';

contract('ExternalCall', function (accounts) {
    const [operator, tokenHolder, tokenBatchHolder, ...otherAccounts] = accounts;

    beforeEach(async function () {
        this.externalCall = await ExternalCall.new();
        this.fortyTwo = await FortyTwo.new();
        this.revert = await Revert.new();
    });

    describe('call_forty_two', function () {
        it('returns 42', async function () {
            expect(await this.externalCall.call_forty_two(this.fortyTwo.address)).to.be.bignumber.equal('42');
        });
    });

    describe('call_revertWithMessage', function () {
        it('reverts with error message', async function () {
            await expectRevert(
                this.externalCall.call_revertWithMessage(this.revert.address),
                'error message',
            );
        });
    });

    describe('try_call_forty_two', function () {
        it('returns 42', async function () {
            expect(await this.externalCall.try_call_forty_two(this.fortyTwo.address)).to.be.bignumber.equal('42');
        });

        it('revert with not implemented', async function () {
             await expectRevert(
                 this.externalCall.try_call_forty_two(this.revert.address),
                 'not implemented',
             );
        });
    });

    describe('try_call_revertWithMessage', function () {
        it('callee reverts with error message', async function () {
            await expectRevert(
                this.externalCall.try_call_revertWithMessage(this.revert.address),
                'error reason',
            );
        });

        it('callee did not implement the function', async function () {
            await expectRevert(
                this.externalCall.try_call_revertWithMessage(this.fortyTwo.address),
                'error data',
            );
        });
    });

    describe('ERC721Receiver', function () {
        it('receiver without any error', async function () {
            const tokenId = 5042;
            const data = '0x42';
            const receiver = await ERC721Receiver.new(RECEIVER_MAGIC_VALUE, Error.None);
            await expectRevert(
                this.externalCall.doSafeTransferAcceptanceCheck(this.externalCall.address, receiver.address, tokenId, data),
                'ok',
            );
        });
        it('receiver reverting with error message', async function () {
            const tokenId = 5042;
            const data = '0x42';
            const receiver = await ERC721Receiver.new(RECEIVER_MAGIC_VALUE, Error.RevertWithMessage);
            await expectRevert(
                this.externalCall.doSafeTransferAcceptanceCheck(this.externalCall.address, receiver.address, tokenId, data),
                'ERC721ReceiverMock: reverting',
            );
        });
        it('receiver reverting without error message', async function () {
            const tokenId = 5042;
            const data = '0x42';
            const receiver = await ERC721Receiver.new(RECEIVER_MAGIC_VALUE, Error.RevertWithoutMessage);
            await expectRevert(
                this.externalCall.doSafeTransferAcceptanceCheck(this.externalCall.address, receiver.address, tokenId, data),
                'err_data',
            );
        });
        it('receiver panicking', async function () {
            const tokenId = 5042;
            const data = '0x42';
            const receiver = await ERC721Receiver.new(RECEIVER_MAGIC_VALUE, Error.Panic);
            await expectRevert(
                this.externalCall.doSafeTransferAcceptanceCheck(this.externalCall.address, receiver.address, tokenId, data),
                'panic',
            );
        });
    });

    describe('ERC1155Receiver', function () {
        it('receiver without any error', async function () {
            const ids = [1,2,3];
            const amounts = [10, 100, 1000];
            const data = '0x42';
            const receiver = await ERC1155Receiver.new(RECEIVER_SINGLE_MAGIC_VALUE, false, RECEIVER_BATCH_MAGIC_VALUE, false);
            await expectRevert(
                this.externalCall.doSafeBatchTransferAcceptanceCheck(this.externalCall.address, this.externalCall.address, receiver.address, ids, amounts, data),
                'ok',
            );
        });
        // TODO: Error with "Transaction ran out of gas"
        it('receiver reverting with error message', async function () {
             const ids = [1,2,3];
             const amounts = [10, 100, 1000];
             const data = '0x42';
             const receiver = await ERC1155Receiver.new(RECEIVER_SINGLE_MAGIC_VALUE, false, RECEIVER_BATCH_MAGIC_VALUE, true);
             await expectRevert(
                 this.externalCall.doSafeBatchTransferAcceptanceCheck(this.externalCall.address, this.externalCall.address, receiver.address, ids, amounts, data),
                 'err_reason',
             );
         });
        // TODO: Error with "Transaction ran out of gas"
        it('receiver reverting without error message', async function () {
             const ids = [1,2,3];
             const amounts = [10, 100, 1000];
             const data = '0x42';
             const receiver = await ERC721Receiver.new(RECEIVER_MAGIC_VALUE, Error.Panic);
             await expectRevert(
                 this.externalCall.doSafeBatchTransferAcceptanceCheck(this.externalCall.address, this.externalCall.address, receiver.address, ids, amounts, data),
                 'err_data',
             );
         });
    });
});
