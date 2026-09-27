import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

test('vendored hash sources match the pinned upstream integrity receipt', async () => {
  const root = new URL('../vendor/noble-hashes/', import.meta.url);
  const receipt = JSON.parse(await readFile(new URL('provenance.json', root), 'utf8'));
  for (const [name, hash] of Object.entries(receipt.files)) {
    const bytes = await readFile(new URL(name, root));
    assert.equal(createHash('sha256').update(bytes).digest('hex'), hash, name);
  }
});
