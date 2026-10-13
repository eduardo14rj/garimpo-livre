import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const script = readFileSync(new URL('../src-tauri/src/coleta/login_navigation.js', import.meta.url), 'utf8');
const diagnostic = JSON.parse(readFileSync(new URL('../src-tauri/tests/fixtures/login-diagnostico.json', import.meta.url), 'utf8'));

class FakeAnchor {
  constructor(href, attributes = {}) {
    this.href = href;
    this.attributes = attributes;
  }
  hasAttribute(name) { return Object.hasOwn(this.attributes, name); }
  getAttribute(name) { return this.attributes[name] ?? null; }
  closest() { return null; }
}

function page(href, pageUrl, attributes = {}) {
  const handlers = {};
  const navigations = [];
  const location = {
    href: pageUrl,
    pathname: new URL(pageUrl).pathname,
    assign: (destination) => navigations.push(destination),
  };
  const window = {
    addEventListener: (name, handler, capture) => { handlers[name] = { handler, capture }; },
  };
  runInNewContext(script, { window, location, URL, HTMLAnchorElement: FakeAnchor });
  const link = new FakeAnchor(href, attributes);
  return {
    location,
    link,
    navigations,
    handler: handlers.click.handler,
    capture: handlers.click.capture,
  };
}

function click(handler, overrides = {}) {
  const event = {
    button: 0,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    defaultPrevented: false,
    propagationStopped: false,
    composedPath: () => [{ tagName: 'SPAN' }, event.link],
    preventDefault() { this.defaultPrevented = true; },
    stopImmediatePropagation() { this.propagationStopped = true; },
    ...overrides,
  };
  handler(event);
  return event;
}

test('reproduz o diagnóstico enviado e navega pelo mesmo WebView sem ponte IPC', () => {
  const testPage = page(diagnostic.href, diagnostic.page);
  const event = click(testPage.handler, { link: testPage.link });
  assert.equal(testPage.capture, true);
  assert.equal(event.defaultPrevented, true);
  assert.equal(event.propagationStopped, true);
  assert.deepEqual(testPage.navigations, [diagnostic.href]);
});

test('preserva query string e navega sem trocar de janela', () => {
  const href = 'https://www.mercadolivre.com/jms/mlb/lgz/login?go=https%3A%2F%2Flista.mercadolivre.com.br%2Fcapa';
  const testPage = page(href, diagnostic.page);
  click(testPage.handler, { link: testPage.link });
  assert.deepEqual(testPage.navigations, [href]);
});

test('não interfere em teclas modificadoras, links desabilitados ou outras páginas', () => {
  const cases = [
    page(diagnostic.href, diagnostic.page, { 'aria-disabled': 'true' }),
    page(diagnostic.href, 'https://www.mercadolivre.com.br/home'),
    page(diagnostic.href, diagnostic.page),
  ];
  click(cases[0].handler, { link: cases[0].link });
  click(cases[1].handler, { link: cases[1].link });
  click(cases[2].handler, { link: cases[2].link, ctrlKey: true });
  assert.deepEqual(cases.map((item) => item.navigations), [[], [], []]);
});

test('bloqueia domínios parecidos, HTTP e destinos que não são login', () => {
  for (const href of [
    'https://mercadolivre.com.evil.invalid/jms/mlb/lgz/login',
    'http://www.mercadolivre.com/jms/mlb/lgz/login',
    'https://www.mercadolivre.com/jms/mlb/lgz/registration',
    'https://usuario:senha@www.mercadolivre.com/jms/mlb/lgz/login',
  ]) {
    const testPage = page(href, diagnostic.page);
    click(testPage.handler, { link: testPage.link });
    assert.deepEqual(testPage.navigations, [], href);
  }
});
