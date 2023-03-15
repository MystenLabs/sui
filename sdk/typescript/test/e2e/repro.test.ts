import { expect, it } from 'vitest';
import { Transaction } from '../../src';
import { setup } from './utils/setup';

it('repro', async () => {
  const toolbox = await setup();
  const tx = new Transaction();

  const MAX_BUDGET = 10000000000n;

  const coin = tx.splitCoin(tx.gas, tx.pure(MAX_BUDGET));
  tx.transferObjects([coin], tx.pure(toolbox.address()));
  // Explicitly set payment to empty vector:
  tx.setGasPayment([]);
  tx.setGasBudget(MAX_BUDGET);

  const result = await toolbox.signer.dryRunTransaction({ transaction: tx });
  console.log(result);
  expect(result.effects.status.status).toBe('success');
});
