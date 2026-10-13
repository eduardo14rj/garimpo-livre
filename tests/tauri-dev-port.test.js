import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:net';
import test from 'node:test';
import { configForDevPort, findAvailablePort, portAvailable } from '../scripts/tauri-dev-port.mjs';

test('escolhe uma porta livre sem substituir a porta que está ocupada', async () => {
  const occupied = new Set([1420, 1421, 1423]);
  const port = await findAvailablePort(1420, 1430, 2, async (candidate) => !occupied.has(candidate));
  assert.equal(port, 1424);
});

test('não escolhe uma porta quando nenhum par consecutivo está livre', async () => {
  const port = await findAvailablePort(80, 82, 2, async () => false);
  assert.equal(port, null);
});

test('detecta uma porta ocupada e volta a liberá-la após fechar o servidor local', async () => {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const { port } = server.address();
  try {
    assert.equal(await portAvailable(port), false);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
  assert.equal(await portAvailable(port), true);
});

test('detecta uma porta ocupada somente no loopback IPv6', async (context) => {
  const server = createServer();
  try {
    await new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(0, '::1', resolve);
    });
  } catch (error) {
    context.skip(`Loopback IPv6 indisponível: ${error.code}`);
    return;
  }
  const { port } = server.address();
  try {
    assert.equal(await portAvailable(port), false);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
  assert.equal(await portAvailable(port), true);
});

test('perfil isolado só é criado quando o desenvolvimento precisa trocar de porta', () => {
  assert.deepEqual(configForDevPort(1420), {
    build: { devUrl: 'http://localhost:1420' },
  });
  assert.deepEqual(configForDevPort(1424, true), {
    identifier: 'com.eduardo.garimpolivre.dev1424',
    build: { devUrl: 'http://localhost:1424' },
  });
});

test('runner Tauri usa a porta alternativa e o perfil isolado sem iniciar a janela do app', async (context) => {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const { port } = server.address();
  try {
    if (port >= 65534) {
      context.skip('A porta temporária não deixa espaço para selecionar a próxima.');
      return;
    }
    const result = spawnSync(process.execPath, ['scripts/tauri.mjs', 'dev', '--help'], {
      cwd: new URL('..', import.meta.url),
      encoding: 'utf8',
      env: { ...process.env, GARIMPO_TAURI_DEV_PORT: String(port) },
    });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, new RegExp(`Porta ${port} ocupada; usando \\d+ e um perfil isolado`));
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});
