const { BN, constants, expectEvent, expectRevert, time } = require('@openzeppelin/test-helpers');
const { ZERO_ADDRESS, ZERO_BYTES32 } = constants;
const { proposalStatesToBitMap } = require('../helpers/governance');

const { expect } = require('chai');

const { shouldSupportInterfaces } = require('../utils/introspection/SupportsInterface.behavior');
const { expectRevertCustomError } = require('../helpers/customError');
const { OperationState } = require('../helpers/enums');

const TimelockController = artifacts.require('TimelockController');
const CallReceiverMock = artifacts.require('CallReceiverMock');
const Implementation2 = artifacts.require('Implementation2');
const ERC721 = artifacts.require('$ERC721');
const ERC1155 = artifacts.require('$ERC1155');
const TimelockReentrant = artifacts.require('$TimelockReentrant');

const MINDELAY = time.duration.days(1);

const salt = '0x025e7b0be353a74631ad648c667493c0e1cd31caa4cc2d3520fdc171ea0cc726'; // a random value

function genOperation(target, value, data, predecessor, salt) {
  const id = web3.utils.keccak256(
    web3.eth.abi.encodeParameters(
      ['address', 'uint256', 'bytes', 'uint256', 'bytes32'],
      [target, value, data, predecessor, salt],
    ),
  );
  return { id, target, value, data, predecessor, salt };
}

function genOperationBatch(targets, values, payloads, predecessor, salt) {
  const id = web3.utils.keccak256(
    web3.eth.abi.encodeParameters(
      ['address[]', 'uint256[]', 'bytes[]', 'uint256', 'bytes32'],
      [targets, values, payloads, predecessor, salt],
    ),
  );
  return { id, targets, values, payloads, predecessor, salt };
}

contract('TimelockController', function (accounts) {
  const [, admin, proposer, canceller, executor, other] = accounts;

  const DEFAULT_ADMIN_ROLE = '0x0000000000000000000000000000000000000000000000000000000000000000';
  const PROPOSER_ROLE = web3.utils.soliditySha3('PROPOSER_ROLE');
  const EXECUTOR_ROLE = web3.utils.soliditySha3('EXECUTOR_ROLE');
  const CANCELLER_ROLE = web3.utils.soliditySha3('CANCELLER_ROLE');

  beforeEach(async function () {
    // Deploy new timelock
    this.mock = await TimelockController.new(MINDELAY, [proposer], [executor], admin);

    expect(await this.mock.hasRole(CANCELLER_ROLE, proposer)).to.be.equal(true);
    await this.mock.revokeRole(CANCELLER_ROLE, proposer, { from: admin });
    await this.mock.grantRole(CANCELLER_ROLE, canceller, { from: admin });

    // Mocks
    this.callreceivermock = await CallReceiverMock.new({ from: admin });
    this.implementation2 = await Implementation2.new({ from: admin });
  });

  shouldSupportInterfaces(['ERC1155Receiver']);

  it('initial state', async function () {
    expect(await this.mock.getMinDelay()).to.be.bignumber.equal(MINDELAY);

    expect(await this.mock.DEFAULT_ADMIN_ROLE()).to.be.equal(DEFAULT_ADMIN_ROLE);
    expect(await this.mock.PROPOSER_ROLE()).to.be.equal(PROPOSER_ROLE);
    expect(await this.mock.EXECUTOR_ROLE()).to.be.equal(EXECUTOR_ROLE);
    expect(await this.mock.CANCELLER_ROLE()).to.be.equal(CANCELLER_ROLE);

    expect(
      await Promise.all([PROPOSER_ROLE, CANCELLER_ROLE, EXECUTOR_ROLE].map(role => this.mock.hasRole(role, proposer))),
    ).to.be.deep.equal([true, false, false]);

    expect(
      await Promise.all([PROPOSER_ROLE, CANCELLER_ROLE, EXECUTOR_ROLE].map(role => this.mock.hasRole(role, canceller))),
    ).to.be.deep.equal([false, true, false]);

    expect(
      await Promise.all([PROPOSER_ROLE, CANCELLER_ROLE, EXECUTOR_ROLE].map(role => this.mock.hasRole(role, executor))),
    ).to.be.deep.equal([false, false, true]);
  });

  it('optional admin', async function () {
    const mock = await TimelockController.new(MINDELAY, [proposer], [executor], ZERO_ADDRESS, { from: other });

    expect(await mock.hasRole(DEFAULT_ADMIN_ROLE, admin)).to.be.equal(false);
    expect(await mock.hasRole(DEFAULT_ADMIN_ROLE, mock.address)).to.be.equal(true);
  });

  describe('methods', function () {
    describe('operation hashing', function () {
      it('hashOperation', async function () {
        this.operation = genOperation(
          '0x29cebefe301c6ce1bb36b58654fea275e1cacc83',
          '0xf94fdd6e21da21d2',
          '0xa3bc5104',
          '0xba41db3be0a9929145cfe480bd0f1f003689104d275ae912099f925df424ef94',
          '0x60d9109846ab510ed75c15f979ae366a8a2ace11d34ba9788c13ac296db50e6e',
        );
        expect(
          await this.mock.hashOperation(
            this.operation.target,
            this.operation.value,
            this.operation.data,
            this.operation.predecessor,
            this.operation.salt,
          ),
        ).to.be.equal(this.operation.id);
      });

      it('hashOperationBatch', async function () {
        this.operation = genOperationBatch(
          Array(8).fill('0x2d5f21620e56531c1d59c2df9b8e95d129571f71'),
          Array(8).fill('0x2b993cfce932ccee'),
          Array(8).fill('0xcf51966b'),
          '0xce8f45069cc71d25f71ba05062de1a3974f9849b004de64a70998bca9d29c2e7',
          '0x8952d74c110f72bfe5accdf828c74d53a7dfb71235dfa8a1e8c75d8576b372ff',
        );
        expect(
          await this.mock.hashOperationBatch(
            this.operation.targets,
            this.operation.values,
            this.operation.payloads,
            this.operation.predecessor,
            this.operation.salt,
          ),
        ).to.be.equal(this.operation.id);
      });
    });
    describe('simple', function () {
      describe('schedule', function () {
        beforeEach(async function () {
          this.operation = genOperation(
            '0x31754f590B97fD975Eb86938f18Cc304E264D2F2',
            0,
            '0x3bf92ccc',
            ZERO_BYTES32,
            salt,
          );
        });

        it('proposer can schedule', async function () {
          const receipt = await this.mock.schedule(
            this.operation.target,
            this.operation.value,
            this.operation.data,
            this.operation.predecessor,
            this.operation.salt,
            MINDELAY,
            { from: proposer },
          );
          expectEvent(receipt, 'CallScheduled', {
            id: this.operation.id,
            index: web3.utils.toBN(0),
            target: this.operation.target,
            value: web3.utils.toBN(this.operation.value),
            data: this.operation.data,
            predecessor: this.operation.predecessor,
            delay: MINDELAY,
          });

          expectEvent(receipt, 'CallSalt', {
            id: this.operation.id,
            salt: this.operation.salt,
          });

          const block = await web3.eth.getBlock(receipt.receipt.blockHash);

          expect(await this.mock.getTimestamp(this.operation.id)).to.be.bignumber.equal(
            web3.utils.toBN(block.timestamp).add(MINDELAY),
          );
        });

        it('prevent overwriting active operation', async function () {
          await this.mock.schedule(
            this.operation.target,
            this.operation.value,
            this.operation.data,
            this.operation.predecessor,
            this.operation.salt,
            MINDELAY,
            { from: proposer },
          );

          await expectRevertCustomError(
            this.mock.schedule(
              this.operation.target,
              this.operation.value,
              this.operation.data,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY,
              { from: proposer },
            ),
            'TimelockUnexpectedOperationState',
            [this.operation.id, proposalStatesToBitMap(OperationState.Unset)],
          );
        });

        it('prevent non-proposer from committing', async function () {
          await expectRevertCustomError(
            this.mock.schedule(
              this.operation.target,
              this.operation.value,
              this.operation.data,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY,
              { from: other },
            ),
            `AccessControlUnauthorizedAccount`,
            [other, PROPOSER_ROLE],
          );
        });

        it('enforce minimum delay', async function () {
          await expectRevertCustomError(
            this.mock.schedule(
              this.operation.target,
              this.operation.value,
              this.operation.data,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY - 1,
              { from: proposer },
            ),
            'TimelockInsufficientDelay',
            [MINDELAY, MINDELAY - 1],
          );
        });

        it('schedule operation with salt zero', async function () {
          const { receipt } = await this.mock.schedule(
            this.operation.target,
            this.operation.value,
            this.operation.data,
            this.operation.predecessor,
            ZERO_BYTES32,
            MINDELAY,
            { from: proposer },
          );
          expectEvent.notEmitted(receipt, 'CallSalt');
        });
      });

      describe('execute', function () {
        beforeEach(async function () {
          this.operation = genOperation(
            '0xAe22104DCD970750610E6FE15E623468A98b15f7',
            0,
            '0x13e414de',
            ZERO_BYTES32,
            '0xc1059ed2dc130227aa1d1d539ac94c641306905c020436c636e19e3fab56fc7f',
          );
        });

        it('revert if operation is not scheduled', async function () {
          await expectRevertCustomError(
            this.mock.execute(
              this.operation.target,
              this.operation.value,
              this.operation.data,
              this.operation.predecessor,
              this.operation.salt,
              { from: executor },
            ),
            'TimelockUnexpectedOperationState',
            [this.operation.id, proposalStatesToBitMap(OperationState.Ready)],
          );
        });

        describe('with scheduled operation', function () {
          beforeEach(async function () {
            ({ receipt: this.receipt, logs: this.logs } = await this.mock.schedule(
              this.operation.target,
              this.operation.value,
              this.operation.data,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY,
              { from: proposer },
            ));
          });

          it('revert if execution comes too early 1/2', async function () {
            await expectRevertCustomError(
              this.mock.execute(
                this.operation.target,
                this.operation.value,
                this.operation.data,
                this.operation.predecessor,
                this.operation.salt,
                { from: executor },
              ),
              'TimelockUnexpectedOperationState',
              [this.operation.id, proposalStatesToBitMap(OperationState.Ready)],
            );
          });

          it('revert if execution comes too early 2/2', async function () {
            const timestamp = await this.mock.getTimestamp(this.operation.id);
            await time.increaseTo(timestamp - 5); // -1 is too tight, test sometime fails

            await expectRevertCustomError(
              this.mock.execute(
                this.operation.target,
                this.operation.value,
                this.operation.data,
                this.operation.predecessor,
                this.operation.salt,
                { from: executor },
              ),
              'TimelockUnexpectedOperationState',
              [this.operation.id, proposalStatesToBitMap(OperationState.Ready)],
            );
          });

          describe('on time', function () {
            beforeEach(async function () {
              const timestamp = await this.mock.getTimestamp(this.operation.id);
              await time.increaseTo(timestamp);
            });

            it('executor can reveal', async function () {
              const receipt = await this.mock.execute(
                this.operation.target,
                this.operation.value,
                this.operation.data,
                this.operation.predecessor,
                this.operation.salt,
                { from: executor },
              );
              expectEvent(receipt, 'CallExecuted', {
                id: this.operation.id,
                index: web3.utils.toBN(0),
                target: this.operation.target,
                value: web3.utils.toBN(this.operation.value),
                data: this.operation.data,
              });
            });

            it('prevent non-executor from revealing', async function () {
              await expectRevertCustomError(
                this.mock.execute(
                  this.operation.target,
                  this.operation.value,
                  this.operation.data,
                  this.operation.predecessor,
                  this.operation.salt,
                  { from: other },
                ),
                `AccessControlUnauthorizedAccount`,
                [other, EXECUTOR_ROLE],
              );
            });

            it('prevents reentrancy execution', async function () {
              // Create operation
              const reentrant = await TimelockReentrant.new();
              const reentrantOperation = genOperation(
                reentrant.address,
                0,
                reentrant.contract.methods.reenter().encodeABI(),
                ZERO_BYTES32,
                salt,
              );

              // Schedule so it can be executed
              await this.mock.schedule(
                reentrantOperation.target,
                reentrantOperation.value,
                reentrantOperation.data,
                reentrantOperation.predecessor,
                reentrantOperation.salt,
                MINDELAY,
                { from: proposer },
              );

              // Advance on time to make the operation executable
              const timestamp = await this.mock.getTimestamp(reentrantOperation.id);
              await time.increaseTo(timestamp);

              // Grant executor role to the reentrant contract
              await this.mock.grantRole(EXECUTOR_ROLE, reentrant.address, { from: admin });

              // Prepare reenter
              const data = this.mock.contract.methods
                .execute(
                  reentrantOperation.target,
                  reentrantOperation.value,
                  reentrantOperation.data,
                  reentrantOperation.predecessor,
                  reentrantOperation.salt,
                )
                .encodeABI();
              await reentrant.enableRentrancy(this.mock.address, data);

              // Expect to fail
              await expectRevertCustomError(
                this.mock.execute(
                  reentrantOperation.target,
                  reentrantOperation.value,
                  reentrantOperation.data,
                  reentrantOperation.predecessor,
                  reentrantOperation.salt,
                  { from: executor },
                ),
                'TimelockUnexpectedOperationState',
                [reentrantOperation.id, proposalStatesToBitMap(OperationState.Ready)],
              );

              // Disable reentrancy
              await reentrant.disableReentrancy();
              const nonReentrantOperation = reentrantOperation; // Not anymore

              // Try again successfully
              const receipt = await this.mock.execute(
                nonReentrantOperation.target,
                nonReentrantOperation.value,
                nonReentrantOperation.data,
                nonReentrantOperation.predecessor,
                nonReentrantOperation.salt,
                { from: executor },
              );
              expectEvent(receipt, 'CallExecuted', {
                id: nonReentrantOperation.id,
                index: web3.utils.toBN(0),
                target: nonReentrantOperation.target,
                value: web3.utils.toBN(nonReentrantOperation.value),
                data: nonReentrantOperation.data,
              });
            });
          });
        });
      });
    });

    describe('batch', function () {
      describe('schedule', function () {
        beforeEach(async function () {
          this.operation = genOperationBatch(
            Array(8).fill('0xEd912250835c812D4516BBD80BdaEA1bB63a293C'),
            Array(8).fill(0),
            Array(8).fill('0x2fcb7a88'),
            ZERO_BYTES32,
            '0x6cf9d042ade5de78bed9ffd075eb4b2a4f6b1736932c2dc8af517d6e066f51f5',
          );
        });

        it('proposer can schedule', async function () {
          const receipt = await this.mock.scheduleBatch(
            this.operation.targets,
            this.operation.values,
            this.operation.payloads,
            this.operation.predecessor,
            this.operation.salt,
            MINDELAY,
            { from: proposer },
          );
          for (const i in this.operation.targets) {
            expectEvent(receipt, 'CallScheduled', {
              id: this.operation.id,
              index: web3.utils.toBN(i),
              target: this.operation.targets[i],
              value: web3.utils.toBN(this.operation.values[i]),
              data: this.operation.payloads[i],
              predecessor: this.operation.predecessor,
              delay: MINDELAY,
            });

            expectEvent(receipt, 'CallSalt', {
              id: this.operation.id,
              salt: this.operation.salt,
            });
          }

          const block = await web3.eth.getBlock(receipt.receipt.blockHash);

          expect(await this.mock.getTimestamp(this.operation.id)).to.be.bignumber.equal(
            web3.utils.toBN(block.timestamp).add(MINDELAY),
          );
        });

        it('prevent overwriting active operation', async function () {
          await this.mock.scheduleBatch(
            this.operation.targets,
            this.operation.values,
            this.operation.payloads,
            this.operation.predecessor,
            this.operation.salt,
            MINDELAY,
            { from: proposer },
          );

          await expectRevertCustomError(
            this.mock.scheduleBatch(
              this.operation.targets,
              this.operation.values,
              this.operation.payloads,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY,
              { from: proposer },
            ),
            'TimelockUnexpectedOperationState',
            [this.operation.id, proposalStatesToBitMap(OperationState.Unset)],
          );
        });

        it('length of batch parameter must match #1', async function () {
          await expectRevertCustomError(
            this.mock.scheduleBatch(
              this.operation.targets,
              [],
              this.operation.payloads,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY,
              { from: proposer },
            ),
            'TimelockInvalidOperationLength',
            [this.operation.targets.length, this.operation.payloads.length, 0],
          );
        });

        it('length of batch parameter must match #1', async function () {
          await expectRevertCustomError(
            this.mock.scheduleBatch(
              this.operation.targets,
              this.operation.values,
              [],
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY,
              { from: proposer },
            ),
            'TimelockInvalidOperationLength',
            [this.operation.targets.length, 0, this.operation.payloads.length],
          );
        });

        it('prevent non-proposer from committing', async function () {
          await expectRevertCustomError(
            this.mock.scheduleBatch(
              this.operation.targets,
              this.operation.values,
              this.operation.payloads,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY,
              { from: other },
            ),
            `AccessControlUnauthorizedAccount`,
            [other, PROPOSER_ROLE],
          );
        });

        it('enforce minimum delay', async function () {
          await expectRevertCustomError(
            this.mock.scheduleBatch(
              this.operation.targets,
              this.operation.values,
              this.operation.payloads,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY - 1,
              { from: proposer },
            ),
            'TimelockInsufficientDelay',
            [MINDELAY, MINDELAY - 1],
          );
        });
      });

      describe('execute', function () {
        beforeEach(async function () {
          this.operation = genOperationBatch(
            Array(8).fill('0x76E53CcEb05131Ef5248553bEBDb8F70536830b1'),
            Array(8).fill(0),
            Array(8).fill('0x58a60f63'),
            ZERO_BYTES32,
            '0x9545eeabc7a7586689191f78a5532443698538e54211b5bd4d7dc0fc0102b5c7',
          );
        });

        it('revert if operation is not scheduled', async function () {
          await expectRevertCustomError(
            this.mock.executeBatch(
              this.operation.targets,
              this.operation.values,
              this.operation.payloads,
              this.operation.predecessor,
              this.operation.salt,
              { from: executor },
            ),
            'TimelockUnexpectedOperationState',
            [this.operation.id, proposalStatesToBitMap(OperationState.Ready)],
          );
        });

        describe('with scheduled operation', function () {
          beforeEach(async function () {
            ({ receipt: this.receipt, logs: this.logs } = await this.mock.scheduleBatch(
              this.operation.targets,
              this.operation.values,
              this.operation.payloads,
              this.operation.predecessor,
              this.operation.salt,
              MINDELAY,
              { from: proposer },
            ));
          });

          it('revert if execution comes too early 1/2', async function () {
            await expectRevertCustomError(
              this.mock.executeBatch(
                this.operation.targets,
                this.operation.values,
                this.operation.payloads,
                this.operation.predecessor,
                this.operation.salt,
                { from: executor },
              ),
              'TimelockUnexpectedOperationState',
              [this.operation.id, proposalStatesToBitMap(OperationState.Ready)],
            );
          });

          it('revert if execution comes too early 2/2', async function () {
            const timestamp = await this.mock.getTimestamp(this.operation.id);
            await time.increaseTo(timestamp - 5); // -1 is to tight, test sometime fails

            await expectRevertCustomError(
              this.mock.executeBatch(
                this.operation.targets,
                this.operation.values,
                this.operation.payloads,
                this.operation.predecessor,
                this.operation.salt,
                { from: executor },
              ),
              'TimelockUnexpectedOperationState',
              [this.operation.id, proposalStatesToBitMap(OperationState.Ready)],
            );
          });

          describe('on time', function () {
            beforeEach(async function () {
              const timestamp = await this.mock.getTimestamp(this.operation.id);
              await time.increaseTo(timestamp);
            });

            it('executor can reveal', async function () {
              const receipt = await this.mock.executeBatch(
                this.operation.targets,
                this.operation.values,
                this.operation.payloads,
                this.operation.predecessor,
                this.operation.salt,
                { from: executor },
              );
              for (const i in this.operation.targets) {
                expectEvent(receipt, 'CallExecuted', {
                  id: this.operation.id,
                  index: web3.utils.toBN(i),
                  target: this.operation.targets[i],
                  value: web3.utils.toBN(this.operation.values[i]),
                  data: this.operation.payloads[i],
                });
              }
            });

            it('prevent non-executor from revealing', async function () {
              await expectRevertCustomError(
                this.mock.executeBatch(
                  this.operation.targets,
                  this.operation.values,
                  this.operation.payloads,
                  this.operation.predecessor,
                  this.operation.salt,
                  { from: other },
                ),
                `AccessControlUnauthorizedAccount`,
                [other, EXECUTOR_ROLE],
              );
            });

            it('length mismatch #1', async function () {
              await expectRevertCustomError(
                this.mock.executeBatch(
                  [],
                  this.operation.values,
                  this.operation.payloads,
                  this.operation.predecessor,
                  this.operation.salt,
                  { from: executor },
                ),
                'TimelockInvalidOperationLength',
                [0, this.operation.payloads.length, this.operation.values.length],
              );
            });

            it('length mismatch #2', async function () {
              await expectRevertCustomError(
                this.mock.executeBatch(
                  this.operation.targets,
                  [],
                  this.operation.payloads,
                  this.operation.predecessor,
                  this.operation.salt,
                  { from: executor },
                ),
                'TimelockInvalidOperationLength',
                [this.operation.targets.length, this.operation.payloads.length, 0],
              );
            });

            it('length mismatch #3', async function () {
              await expectRevertCustomError(
                this.mock.executeBatch(
                  this.operation.targets,
                  this.operation.values,
                  [],
                  this.operation.predecessor,
                  this.operation.salt,
                  { from: executor },
                ),
                'TimelockInvalidOperationLength',
                [this.operation.targets.length, 0, this.operation.values.length],
              );
            });

            it('prevents reentrancy execution', async function () {
              // Create operation
              const reentrant = await TimelockReentrant.new();
              const reentrantBatchOperation = genOperationBatch(
                [reentrant.address],
                [0],
                [reentrant.contract.methods.reenter().encodeABI()],
                ZERO_BYTES32,
                salt,
              );

              // Schedule so it can be executed
              await this.mock.scheduleBatch(
                reentrantBatchOperation.targets,
                reentrantBatchOperation.values,
                reentrantBatchOperation.payloads,
                reentrantBatchOperation.predecessor,
                reentrantBatchOperation.salt,
                MINDELAY,
                { from: proposer },
              );

              // Advance on time to make the operation executable
              const timestamp = await this.mock.getTimestamp(reentrantBatchOperation.id);
              await time.increaseTo(timestamp);

              // Grant executor role to the reentrant contract
              await this.mock.grantRole(EXECUTOR_ROLE, reentrant.address, { from: admin });

              // Prepare reenter
              const data = this.mock.contract.methods
                .executeBatch(
                  reentrantBatchOperation.targets,
                  reentrantBatchOperation.values,
                  reentrantBatchOperation.payloads,
                  reentrantBatchOperation.predecessor,
                  reentrantBatchOperation.salt,
                )
                .encodeABI();
              await reentrant.enableRentrancy(this.mock.address, data);

              // Expect to fail
              await expectRevertCustomError(
                this.mock.executeBatch(
                  reentrantBatchOperation.targets,
                  reentrantBatchOperation.values,
                  reentrantBatchOperation.payloads,
                  reentrantBatchOperation.predecessor,
                  reentrantBatchOperation.salt,
                  { from: executor },
                ),
                'TimelockUnexpectedOperationState',
                [reentrantBatchOperation.id, proposalStatesToBitMap(OperationState.Ready)],
              );

              // Disable reentrancy
              await reentrant.disableReentrancy();
              const nonReentrantBatchOperation = reentrantBatchOperation; // Not anymore

              // Try again successfully
              const receipt = await this.mock.executeBatch(
                nonReentrantBatchOperation.targets,
                nonReentrantBatchOperation.values,
                nonReentrantBatchOperation.payloads,
                nonReentrantBatchOperation.predecessor,
                nonReentrantBatchOperation.salt,
                { from: executor },
              );
              for (const i in nonReentrantBatchOperation.targets) {
                expectEvent(receipt, 'CallExecuted', {
                  id: nonReentrantBatchOperation.id,
                  index: web3.utils.toBN(i),
                  target: nonReentrantBatchOperation.targets[i],
                  value: web3.utils.toBN(nonReentrantBatchOperation.values[i]),
                  data: nonReentrantBatchOperation.payloads[i],
                });
              }
            });
          });
        });

        it('partial execution', async function () {
          const operation = genOperationBatch(
            [this.callreceivermock.address, this.callreceivermock.address, this.callreceivermock.address],
            [0, 0, 0],
            [
              this.callreceivermock.contract.methods.mockFunction().encodeABI(),
              this.callreceivermock.contract.methods.mockFunctionRevertsNoReason().encodeABI(),
              this.callreceivermock.contract.methods.mockFunction().encodeABI(),
            ],
            ZERO_BYTES32,
            '0x8ac04aa0d6d66b8812fb41d39638d37af0a9ab11da507afd65c509f8ed079d3e',
          );

          await this.mock.scheduleBatch(
            operation.targets,
            operation.values,
            operation.payloads,
            operation.predecessor,
            operation.salt,
            MINDELAY,
            { from: proposer },
          );
          await time.increase(MINDELAY);
          await expectRevertCustomError(
            this.mock.executeBatch(
              operation.targets,
              operation.values,
              operation.payloads,
              operation.predecessor,
              operation.salt,
              { from: executor },
            ),
            'FailedInnerCall',
            [],
          );
        });
      });
    });

    describe('cancel', function () {
      beforeEach(async function () {
        this.operation = genOperation(
          '0xC6837c44AA376dbe1d2709F13879E040CAb653ca',
          0,
          '0x296e58dd',
          ZERO_BYTES32,
          '0xa2485763600634800df9fc9646fb2c112cf98649c55f63dd1d9c7d13a64399d9',
        );
        ({ receipt: this.receipt, logs: this.logs } = await this.mock.schedule(
          this.operation.target,
          this.operation.value,
          this.operation.data,
          this.operation.predecessor,
          this.operation.salt,
          MINDELAY,
          { from: proposer },
        ));
      });

      it('canceller can cancel', async function () {
        const receipt = await this.mock.cancel(this.operation.id, { from: canceller });
        expectEvent(receipt, 'Cancelled', { id: this.operation.id });
      });

      it('cannot cancel invalid operation', async function () {
        await expectRevertCustomError(
          this.mock.cancel(constants.ZERO_BYTES32, { from: canceller }),
          'TimelockUnexpectedOperationState',
          [constants.ZERO_BYTES32, proposalStatesToBitMap([OperationState.Waiting, OperationState.Ready])],
        );
      });

      it('prevent non-canceller from canceling', async function () {
        await expectRevertCustomError(
          this.mock.cancel(this.operation.id, { from: other }),
          `AccessControlUnauthorizedAccount`,
          [other, CANCELLER_ROLE],
        );
      });
    });
  });

  describe('maintenance', function () {
    it('prevent unauthorized maintenance', async function () {
      await expectRevertCustomError(this.mock.updateDelay(0, { from: other }), 'TimelockUnauthorizedCaller', [other]);
    });

    it('timelock scheduled maintenance', async function () {
      const newDelay = time.duration.hours(6);
      const operation = genOperation(
        this.mock.address,
        0,
        this.mock.contract.methods.updateDelay(newDelay.toString()).encodeABI(),
        ZERO_BYTES32,
        '0xf8e775b2c5f4d66fb5c7fa800f35ef518c262b6014b3c0aee6ea21bff157f108',
      );

      await this.mock.schedule(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);
      const receipt = await this.mock.execute(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        { from: executor },
      );
      expectEvent(receipt, 'MinDelayChange', { newDuration: newDelay.toString(), oldDuration: MINDELAY });

      expect(await this.mock.getMinDelay()).to.be.bignumber.equal(newDelay);
    });
  });

  describe('dependency', function () {
    beforeEach(async function () {
      this.operation1 = genOperation(
        '0xdE66bD4c97304200A95aE0AadA32d6d01A867E39',
        0,
        '0x01dc731a',
        ZERO_BYTES32,
        '0x64e932133c7677402ead2926f86205e2ca4686aebecf5a8077627092b9bb2feb',
      );
      this.operation2 = genOperation(
        '0x3c7944a3F1ee7fc8c5A5134ba7c79D11c3A1FCa3',
        0,
        '0x8f531849',
        this.operation1.id,
        '0x036e1311cac523f9548e6461e29fb1f8f9196b91910a41711ea22f5de48df07d',
      );
      await this.mock.schedule(
        this.operation1.target,
        this.operation1.value,
        this.operation1.data,
        this.operation1.predecessor,
        this.operation1.salt,
        MINDELAY,
        { from: proposer },
      );
      await this.mock.schedule(
        this.operation2.target,
        this.operation2.value,
        this.operation2.data,
        this.operation2.predecessor,
        this.operation2.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);
    });

    it('cannot execute before dependency', async function () {
      await expectRevertCustomError(
        this.mock.execute(
          this.operation2.target,
          this.operation2.value,
          this.operation2.data,
          this.operation2.predecessor,
          this.operation2.salt,
          { from: executor },
        ),
        'TimelockUnexecutedPredecessor',
        [this.operation1.id],
      );
    });

    it('can execute after dependency', async function () {
      await this.mock.execute(
        this.operation1.target,
        this.operation1.value,
        this.operation1.data,
        this.operation1.predecessor,
        this.operation1.salt,
        { from: executor },
      );
      await this.mock.execute(
        this.operation2.target,
        this.operation2.value,
        this.operation2.data,
        this.operation2.predecessor,
        this.operation2.salt,
        { from: executor },
      );
    });
  });

  describe('usage scenario', function () {
    this.timeout(10000);

    it('call', async function () {
      const operation = genOperation(
        this.implementation2.address,
        0,
        this.implementation2.contract.methods.setValue(42).encodeABI(),
        ZERO_BYTES32,
        '0x8043596363daefc89977b25f9d9b4d06c3910959ef0c4d213557a903e1b555e2',
      );

      await this.mock.schedule(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);
      await this.mock.execute(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        { from: executor },
      );

      expect(await this.implementation2.getValue()).to.be.bignumber.equal(web3.utils.toBN(42));
    });

    it('call reverting', async function () {
      const operation = genOperation(
        this.callreceivermock.address,
        0,
        this.callreceivermock.contract.methods.mockFunctionRevertsNoReason().encodeABI(),
        ZERO_BYTES32,
        '0xb1b1b276fdf1a28d1e00537ea73b04d56639128b08063c1a2f70a52e38cba693',
      );

      await this.mock.schedule(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);
      await expectRevertCustomError(
        this.mock.execute(operation.target, operation.value, operation.data, operation.predecessor, operation.salt, {
          from: executor,
        }),
        'FailedInnerCall',
        [],
      );
    });

    it('call throw', async function () {
      const operation = genOperation(
        this.callreceivermock.address,
        0,
        this.callreceivermock.contract.methods.mockFunctionThrows().encodeABI(),
        ZERO_BYTES32,
        '0xe5ca79f295fc8327ee8a765fe19afb58f4a0cbc5053642bfdd7e73bc68e0fc67',
      );

      await this.mock.schedule(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);
      // Targeted function reverts with a panic code (0x1) + the timelock bubble the panic code
      await expectRevert.unspecified(
        this.mock.execute(operation.target, operation.value, operation.data, operation.predecessor, operation.salt, {
          from: executor,
        }),
      );
    });

    it('call out of gas', async function () {
      const operation = genOperation(
        this.callreceivermock.address,
        0,
        this.callreceivermock.contract.methods.mockFunctionOutOfGas().encodeABI(),
        ZERO_BYTES32,
        '0xf3274ce7c394c5b629d5215723563a744b817e1730cca5587c567099a14578fd',
      );

      await this.mock.schedule(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);
      await expectRevertCustomError(
        this.mock.execute(operation.target, operation.value, operation.data, operation.predecessor, operation.salt, {
          from: executor,
          gas: '100000',
        }),
        'FailedInnerCall',
        [],
      );
    });

    it('call payable with eth', async function () {
      const operation = genOperation(
        this.callreceivermock.address,
        1,
        this.callreceivermock.contract.methods.mockFunction().encodeABI(),
        ZERO_BYTES32,
        '0x5ab73cd33477dcd36c1e05e28362719d0ed59a7b9ff14939de63a43073dc1f44',
      );

      await this.mock.schedule(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);

      expect(await web3.eth.getBalance(this.mock.address)).to.be.bignumber.equal(web3.utils.toBN(0));
      expect(await web3.eth.getBalance(this.callreceivermock.address)).to.be.bignumber.equal(web3.utils.toBN(0));

      await this.mock.execute(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        { from: executor, value: 1 },
      );

      expect(await web3.eth.getBalance(this.mock.address)).to.be.bignumber.equal(web3.utils.toBN(0));
      expect(await web3.eth.getBalance(this.callreceivermock.address)).to.be.bignumber.equal(web3.utils.toBN(1));
    });

    it('call nonpayable with eth', async function () {
      const operation = genOperation(
        this.callreceivermock.address,
        1,
        this.callreceivermock.contract.methods.mockFunctionNonPayable().encodeABI(),
        ZERO_BYTES32,
        '0xb78edbd920c7867f187e5aa6294ae5a656cfbf0dea1ccdca3751b740d0f2bdf8',
      );

      await this.mock.schedule(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);

      expect(await web3.eth.getBalance(this.mock.address)).to.be.bignumber.equal(web3.utils.toBN(0));
      expect(await web3.eth.getBalance(this.callreceivermock.address)).to.be.bignumber.equal(web3.utils.toBN(0));

      await expectRevertCustomError(
        this.mock.execute(operation.target, operation.value, operation.data, operation.predecessor, operation.salt, {
          from: executor,
        }),
        'FailedInnerCall',
        [],
      );

      expect(await web3.eth.getBalance(this.mock.address)).to.be.bignumber.equal(web3.utils.toBN(0));
      expect(await web3.eth.getBalance(this.callreceivermock.address)).to.be.bignumber.equal(web3.utils.toBN(0));
    });

    it('call reverting with eth', async function () {
      const operation = genOperation(
        this.callreceivermock.address,
        1,
        this.callreceivermock.contract.methods.mockFunctionRevertsNoReason().encodeABI(),
        ZERO_BYTES32,
        '0xdedb4563ef0095db01d81d3f2decf57cf83e4a72aa792af14c43a792b56f4de6',
      );

      await this.mock.schedule(
        operation.target,
        operation.value,
        operation.data,
        operation.predecessor,
        operation.salt,
        MINDELAY,
        { from: proposer },
      );
      await time.increase(MINDELAY);

      expect(await web3.eth.getBalance(this.mock.address)).to.be.bignumber.equal(web3.utils.toBN(0));
      expect(await web3.eth.getBalance(this.callreceivermock.address)).to.be.bignumber.equal(web3.utils.toBN(0));

      await expectRevertCustomError(
        this.mock.execute(operation.target, operation.value, operation.data, operation.predecessor, operation.salt, {
          from: executor,
        }),
        'FailedInnerCall',
        [],
      );

      expect(await web3.eth.getBalance(this.mock.address)).to.be.bignumber.equal(web3.utils.toBN(0));
      expect(await web3.eth.getBalance(this.callreceivermock.address)).to.be.bignumber.equal(web3.utils.toBN(0));
    });
  });

  describe('safe receive', function () {
    describe('ERC721', function () {
      const name = 'Non Fungible Token';
      const symbol = 'NFT';
      const tokenId = new BN(1);

      beforeEach(async function () {
        this.token = await ERC721.new(name, symbol);
        await this.token.$_mint(other, tokenId);
      });

      it('can receive an ERC721 safeTransfer', async function () {
        await this.token.safeTransferFrom(other, this.mock.address, tokenId, { from: other });
      });
    });

    describe('ERC1155', function () {
      const uri = 'https://token-cdn-domain/{id}.json';
      const tokenIds = {
        1: new BN(1000),
        2: new BN(2000),
        3: new BN(3000),
      };

      beforeEach(async function () {
        this.token = await ERC1155.new(uri);
        await this.token.$_mintBatch(other, Object.keys(tokenIds), Object.values(tokenIds), '0x');
      });

      it('can receive ERC1155 safeTransfer', async function () {
        await this.token.safeTransferFrom(
          other,
          this.mock.address,
          ...Object.entries(tokenIds)[0], // id + amount
          '0x',
          { from: other },
        );
      });

      it('can receive ERC1155 safeBatchTransfer', async function () {
        await this.token.safeBatchTransferFrom(
          other,
          this.mock.address,
          Object.keys(tokenIds),
          Object.values(tokenIds),
          '0x',
          { from: other },
        );
      });
    });
  });
});
