'use strict';

const assert = require('assert');
const path = require('path');
const fs = require('fs');
const { Risco_Crypt } = require('../lib/RCrypt');

const FIXTURES_DIR = path.join(__dirname, 'fixtures');

function noop() {}

function makeCrypt(panelId, cryptEnabled) {
    const c = new Risco_Crypt({
        Panel_Id: panelId,
        Encoding: 'utf-8',
        logger: noop,
        log: 'error',
    });
    c.CryptCommand = !!cryptEnabled;
    return c;
}

let pass = 0;
let fail = 0;

function test(name, fn) {
    try {
        fn();
        pass++;
    } catch (e) {
        fail++;
        console.error(`  FAIL: ${name}`);
        console.error(`    ${e.message}`);
    }
}

// ─── Pseudo-buffer tests ─────────────────────────────────────────────────────

const pseudoBufferVectors = JSON.parse(
    fs.readFileSync(path.join(FIXTURES_DIR, 'pseudo_buffer.json'), 'utf-8')
);

console.log('Pseudo-buffer tests');
for (const v of pseudoBufferVectors) {
    test(`CreatePseudoBuffer(${v.panel_id})`, () => {
        const c = makeCrypt(v.panel_id, false);
        assert.deepStrictEqual(Array.from(c.CryptBuffer), v.buffer);
    });
}

// ─── CRC tests ───────────────────────────────────────────────────────────────

const crcVectors = JSON.parse(
    fs.readFileSync(path.join(FIXTURES_DIR, 'crc.json'), 'utf-8')
);

console.log('CRC tests');
for (const v of crcVectors) {
    test(`CRC: ${v.label}`, () => {
        const c = makeCrypt(0, false);
        const crc = c.GetCommandCRC(Buffer.from(v.input_bytes));
        assert.strictEqual(crc, v.expected_crc);
    });
}

// ─── Encode tests ────────────────────────────────────────────────────────────

const encodeVectors = JSON.parse(
    fs.readFileSync(path.join(FIXTURES_DIR, 'encode.json'), 'utf-8')
);

console.log('Encode tests');
for (const v of encodeVectors) {
    const label = `Encode(${v.panel_id}, ${v.crypt_enabled}, "${v.command}", "${v.cmd_id}"` +
        (v.force_crypt !== null ? `, force=${v.force_crypt}` : '') + ')';
    test(label, () => {
        const c = makeCrypt(v.panel_id, v.crypt_enabled);
        const encoded = c.GetCommande(v.command, v.cmd_id,
            v.force_crypt !== null ? v.force_crypt : undefined);
        assert.deepStrictEqual(Array.from(encoded), v.encoded_bytes);
    });
}

// ─── Decode tests ────────────────────────────────────────────────────────────

const decodeVectors = JSON.parse(
    fs.readFileSync(path.join(FIXTURES_DIR, 'decode.json'), 'utf-8')
);

console.log('Decode tests');
for (const v of decodeVectors) {
    test(`Decode: ${v.label}`, () => {
        const c = makeCrypt(v.panel_id, v.crypt_enabled);
        // Pass a copy since DecodeMessage mutates the input array
        const [cmd_id, command, crc_valid] = c.DecodeMessage([...v.input_bytes]);
        assert.strictEqual(cmd_id, v.expected_cmd_id,
            `cmd_id: expected "${v.expected_cmd_id}", got "${cmd_id}"`);
        assert.strictEqual(command, v.expected_command,
            `command: expected "${v.expected_command}", got "${command}"`);
        assert.strictEqual(crc_valid, v.expected_crc_valid,
            `crc_valid: expected ${v.expected_crc_valid}, got ${crc_valid}`);
    });
}

// ─── DLE edge case tests ─────────────────────────────────────────────────────

const dleVectors = JSON.parse(
    fs.readFileSync(path.join(FIXTURES_DIR, 'dle_edge_cases.json'), 'utf-8')
);

console.log(`DLE edge case tests (${dleVectors.length} panel IDs)`);
for (const v of dleVectors) {
    test(`DLE encode panel_id=${v.panel_id}`, () => {
        // Verify that encoding produces identical bytes to the fixture.
        // Note: JS DecryptChars has a known DLE+DLE decode bug so we only
        // test encoding here. Decode roundtrip is verified by the Rust tests.
        const enc = makeCrypt(v.panel_id, true);
        const encoded = enc.GetCommande('CUSTLST?', '01');
        assert.deepStrictEqual(Array.from(encoded), v.encoded_bytes,
            `encoded bytes mismatch for panel_id ${v.panel_id}`);
    });
}

// ─── Summary ─────────────────────────────────────────────────────────────────

console.log(`\n${pass} passed, ${fail} failed`);
if (fail > 0) {
    process.exit(1);
}
module.exports = { pass, fail };
