import test from 'node:test';
import assert from 'node:assert/strict';
import {
  base64Decode, base64Encode, decodeJwt, digest, formatJson, minifyJson, queryJson, regexMatches, urlDecode, urlEncode,
} from '../src/toolkit.ts';

test('json format, minify, query', () => {
  assert.equal(formatJson('{"a":1}'), '{\n  "a": 1\n}');
  assert.equal(minifyJson('{ "a" : [1, 2] }'), '{"a":[1,2]}');
  const doc = '{"n":{"x":7},"tags":["a","b"]}';
  assert.equal(queryJson(doc, 'n.x'), 7);
  assert.equal(queryJson(doc, '$.tags[1]'), 'b');
  assert.deepEqual(queryJson(doc, ''), JSON.parse(doc));
  assert.throws(() => queryJson(doc, 'n.nope'), /not found/);
  assert.throws(() => formatJson('{bad'));
});

test('regex matches with groups and empty matches', () => {
  const m = regexMatches(String.raw`(?<user>\w+)@(\w+)\.com`, 'g', 'a@example.com b@example.com');
  assert.equal(m.length, 2);
  assert.equal(m[1].named.user, 'b');
  assert.equal(regexMatches('x*', 'g', 'aa').length, 3); // empty matches terminate
  assert.throws(() => regexMatches('(', '', 'x'));
});

test('hashes match known vectors', async () => {
  assert.equal(await digest('SHA-256', 'abc'), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  assert.equal(await digest('SHA-1', 'abc'), 'a9993e364706816aba3e25717850c26c9cd0d89d');
});

test('base64 and url round trips, unicode safe', () => {
  const s = 'Merhaba dünya ş/ğ? €';
  assert.equal(base64Decode(base64Encode(s)), s);
  assert.equal(base64Decode(base64Encode(s, true)), s);
  assert.equal(base64Encode('hi?>', true), 'aGk_Pg');
  assert.equal(urlDecode(urlEncode(s)), s);
  assert.equal(urlDecode('a+b%20c'), 'a b c');
  assert.throws(() => base64Decode('@@@'));
});

test('jwt decoding (no verification) and expiry', () => {
  const enc = (o) => base64Encode(JSON.stringify(o), true);
  const token = `${enc({ alg: 'HS256', typ: 'JWT' })}.${enc({ sub: '1', exp: 1000 })}.sig`;
  const j = decodeJwt(token, 2_000_000);
  assert.equal(j.payload.sub, '1');
  assert.equal(j.expired, true);
  assert.equal(j.expires, '1970-01-01T00:16:40.000Z');
  assert.equal(decodeJwt(token, 500).expired, false);
  assert.throws(() => decodeJwt('a.b'), /three/);
});
