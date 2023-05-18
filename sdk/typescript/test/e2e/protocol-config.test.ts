import { expect, it } from 'vitest';
import { setup } from './utils/setup';

it('can fetch protocol config', async () => {
  const toolbox = await setup();
  const config = await toolbox.provider.getProtocolConfig();
  expect(config).toBeTypeOf('object');
});
