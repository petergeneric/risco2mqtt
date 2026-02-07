'use strict';

const path = require('path');
const fs = require('fs');
const { Risco_Crypt } = require('../lib/RCrypt');

const FIXTURES_DIR = path.join(__dirname, 'fixtures');

// Ensure fixtures directory exists
if (!fs.existsSync(FIXTURES_DIR)) {
    fs.mkdirSync(FIXTURES_DIR, { recursive: true });
}

// Noop logger matching RCrypt's expected signature
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

// ─── 1. Pseudo-buffer vectors ─────────────────────────────────────────────────

function generatePseudoBufferVectors() {
    const panelIds = [0, 1, 2, 100, 255, 256, 1234, 5678, 9999];
    const vectors = panelIds.map(id => {
        const c = makeCrypt(id, false);
        return {
            panel_id: id,
            buffer: Array.from(c.CryptBuffer),
        };
    });
    fs.writeFileSync(
        path.join(FIXTURES_DIR, 'pseudo_buffer.json'),
        JSON.stringify(vectors, null, 2)
    );
    console.log(`  pseudo_buffer.json: ${vectors.length} vectors`);
}

// ─── 2. CRC vectors ──────────────────────────────────────────────────────────

function generateCrcVectors() {
    const c = makeCrypt(0, false);
    const testInputs = [
        { label: 'empty', bytes: [] },
        { label: 'A', bytes: [0x41] },
        { label: '01RMT=5678\\x17', bytes: Array.from(Buffer.from('01RMT=5678\x17')) },
        { label: '01CLOCK\\x17', bytes: Array.from(Buffer.from('01CLOCK\x17')) },
        { label: 'bytes_with_specials', bytes: [0x02, 0x03, 0x10, 0x11, 0x17] },
        { label: 'long_sequence', bytes: Array.from({ length: 256 }, (_, i) => i) },
        { label: '01ACK\\x17', bytes: Array.from(Buffer.from('01ACK\x17')) },
        { label: '01LCL\\x17', bytes: Array.from(Buffer.from('01LCL\x17')) },
        { label: 'N01\\x17', bytes: Array.from(Buffer.from('N01\x17')) },
        { label: 'BCK2\\x17', bytes: Array.from(Buffer.from('BCK2\x17')) },
    ];
    const vectors = testInputs.map(({ label, bytes }) => ({
        label,
        input_bytes: bytes,
        expected_crc: c.GetCommandCRC(Buffer.from(bytes)),
    }));
    fs.writeFileSync(
        path.join(FIXTURES_DIR, 'crc.json'),
        JSON.stringify(vectors, null, 2)
    );
    console.log(`  crc.json: ${vectors.length} vectors`);
}

// ─── 3. Encode vectors ───────────────────────────────────────────────────────

function generateEncodeVectors() {
    const cases = [
        // (panel_id, crypt_enabled, command, cmd_id)
        { panel_id: 0, crypt: false, cmd: 'RMT=5678', cmd_id: '01' },
        { panel_id: 0, crypt: false, cmd: 'LCL', cmd_id: '01' },
        { panel_id: 0, crypt: false, cmd: 'ACK', cmd_id: '01' },
        { panel_id: 0, crypt: false, cmd: 'CLOCK', cmd_id: '01' },
        { panel_id: 1, crypt: true, cmd: 'RMT=5678', cmd_id: '01' },
        { panel_id: 1, crypt: true, cmd: 'CLOCK', cmd_id: '05' },
        { panel_id: 1234, crypt: true, cmd: 'RMT=5678', cmd_id: '01' },
        { panel_id: 1234, crypt: true, cmd: 'LCL', cmd_id: '02' },
        { panel_id: 1234, crypt: true, cmd: 'ACK', cmd_id: '03' },
        { panel_id: 1234, crypt: true, cmd: 'CLOCK', cmd_id: '04' },
        { panel_id: 1234, crypt: true, cmd: 'CUSTLST?', cmd_id: '05' },
        { panel_id: 1234, crypt: true, cmd: 'ZSTT*1:32?', cmd_id: '06' },
        { panel_id: 1234, crypt: true, cmd: 'ARM=1', cmd_id: '07' },
        { panel_id: 1234, crypt: true, cmd: 'DISARM=1', cmd_id: '08' },
        { panel_id: 1234, crypt: true, cmd: 'ZBYPAS=5', cmd_id: '09' },
        { panel_id: 1234, crypt: true, cmd: 'ACTUO3', cmd_id: '10' },
        { panel_id: 5678, crypt: true, cmd: 'RMT=5678', cmd_id: '01' },
        { panel_id: 9999, crypt: true, cmd: 'RMT=5678', cmd_id: '01' },
        { panel_id: 9999, crypt: true, cmd: 'CUSTLST?', cmd_id: '49' },
        // Force-crypt overrides
        { panel_id: 1234, crypt: false, cmd: 'RMT=5678', cmd_id: '01', force_crypt: true },
        { panel_id: 1234, crypt: true, cmd: 'RMT=5678', cmd_id: '01', force_crypt: false },
    ];

    const vectors = cases.map(({ panel_id, crypt, cmd, cmd_id, force_crypt }) => {
        const c = makeCrypt(panel_id, crypt);
        const encoded = c.GetCommande(cmd, cmd_id, force_crypt);
        return {
            panel_id,
            crypt_enabled: crypt,
            command: cmd,
            cmd_id,
            force_crypt: force_crypt !== undefined ? force_crypt : null,
            encoded_bytes: Array.from(encoded),
        };
    });

    fs.writeFileSync(
        path.join(FIXTURES_DIR, 'encode.json'),
        JSON.stringify(vectors, null, 2)
    );
    console.log(`  encode.json: ${vectors.length} vectors`);
}

// ─── 4. Decode vectors ───────────────────────────────────────────────────────

function generateDecodeVectors() {
    const vectors = [];

    // Helper: encode then decode with same key (should succeed)
    function addRoundtrip(panel_id, crypt, cmd, cmd_id) {
        const enc = makeCrypt(panel_id, crypt);
        const encoded = enc.GetCommande(cmd, cmd_id);
        const dec = makeCrypt(panel_id, crypt);
        const [decoded_cmd_id, decoded_cmd, crc_valid] = dec.DecodeMessage(Array.from(encoded));
        vectors.push({
            label: `roundtrip_${panel_id}_${crypt ? 'enc' : 'plain'}_${cmd}`,
            panel_id,
            crypt_enabled: crypt,
            input_bytes: Array.from(encoded),
            expected_cmd_id: decoded_cmd_id,
            expected_command: decoded_cmd,
            expected_crc_valid: crc_valid,
        });
    }

    // Roundtrips
    addRoundtrip(0, false, 'RMT=5678', '01');
    addRoundtrip(0, false, 'CLOCK', '01');
    addRoundtrip(1, true, 'RMT=5678', '01');
    addRoundtrip(1234, true, 'RMT=5678', '01');
    addRoundtrip(1234, true, 'CUSTLST?', '05');
    addRoundtrip(1234, true, 'ZSTT*1:32?', '06');
    addRoundtrip(5678, true, 'ACK', '01');
    addRoundtrip(9999, true, 'CLOCK', '49');

    // Simulate error responses (unencrypted)
    // Build error response frames like the panel would send them
    function addErrorFrame(errorCode) {
        const c = makeCrypt(0, false);
        // Build: STX + errorCode + SEP + CRC + ETX (no encryption indicator, no cmd_id)
        // The panel sends errors unencrypted with no cmd_id prefix
        c.CryptPos = 0;
        c.CryptCommand = false;
        const fullCmd = errorCode + String.fromCharCode(0x17);
        const cmdBytes = c.StringToByte(fullCmd);
        const crc = c.GetCommandCRC(cmdBytes);
        const frame = [0x02].concat(
            Array.from(cmdBytes),
            Array.from(c.StringToByte(crc)),
            [0x03]
        );
        const dec = makeCrypt(0, false);
        const [decoded_cmd_id, decoded_cmd, crc_valid] = dec.DecodeMessage(frame);
        vectors.push({
            label: `error_${errorCode}`,
            panel_id: 0,
            crypt_enabled: false,
            input_bytes: frame,
            expected_cmd_id: decoded_cmd_id,
            expected_command: decoded_cmd,
            expected_crc_valid: crc_valid,
        });
    }

    addErrorFrame('N01');
    addErrorFrame('N04');
    addErrorFrame('BCK2');

    // Wrong-key decode (encrypted with 1234, decoded with 5678)
    {
        const enc = makeCrypt(1234, true);
        const encoded = enc.GetCommande('CUSTLST?', '01');
        const dec = makeCrypt(5678, true);
        const [decoded_cmd_id, decoded_cmd, crc_valid] = dec.DecodeMessage(Array.from(encoded));
        vectors.push({
            label: 'wrong_key_1234_as_5678',
            panel_id: 5678, // decoding panel id
            crypt_enabled: true,
            input_bytes: Array.from(encoded),
            expected_cmd_id: decoded_cmd_id,
            expected_command: decoded_cmd,
            expected_crc_valid: crc_valid,
        });
    }

    fs.writeFileSync(
        path.join(FIXTURES_DIR, 'decode.json'),
        JSON.stringify(vectors, null, 2)
    );
    console.log(`  decode.json: ${vectors.length} vectors`);
}

// ─── 5. DLE edge case vectors ────────────────────────────────────────────────

function generateDleEdgeCaseVectors() {
    // For every panel ID 0-9999, encode a standard command and check if the
    // encoded wire bytes contain DLE escapes for STX (0x02), ETX (0x03), or
    // DLE (0x10) itself. Record only the IDs that produce these escapes.
    const command = 'CUSTLST?';
    const cmd_id = '01';
    const vectors = [];

    for (let id = 0; id <= 9999; id++) {
        const enc = makeCrypt(id, true);
        const encoded = enc.GetCommande(command, cmd_id);
        const bytes = Array.from(encoded);

        // Scan encoded bytes (between STX/CRYPT header and ETX) for DLE escapes
        let hasDleStx = false;
        let hasDleEtx = false;
        let hasDleDle = false;
        // Start after STX (0) and CRYPT (1), stop before ETX (last)
        for (let i = 2; i < bytes.length - 2; i++) {
            if (bytes[i] === 0x10) {
                if (bytes[i + 1] === 0x02) hasDleStx = true;
                else if (bytes[i + 1] === 0x03) hasDleEtx = true;
                else if (bytes[i + 1] === 0x10) hasDleDle = true;
            }
        }

        if (hasDleStx || hasDleEtx || hasDleDle) {
            // Note: JS DecryptChars has a known DLE+DLE decode bug (it mutates
            // input and may double-consume DLE escapes near frame boundaries).
            // We only store encoding results here; decode correctness is
            // verified by the Rust tests which have the fix.
            vectors.push({
                panel_id: id,
                encoded_bytes: bytes,
                has_dle_stx: hasDleStx,
                has_dle_etx: hasDleEtx,
                has_dle_dle: hasDleDle,
            });
        }
    }

    fs.writeFileSync(
        path.join(FIXTURES_DIR, 'dle_edge_cases.json'),
        JSON.stringify(vectors, null, 2)
    );
    console.log(`  dle_edge_cases.json: ${vectors.length} vectors (panel IDs with DLE escapes)`);
}

// ─── Main ────────────────────────────────────────────────────────────────────

console.log('Generating test vectors...');
generatePseudoBufferVectors();
generateCrcVectors();
generateEncodeVectors();
generateDecodeVectors();
generateDleEdgeCaseVectors();
console.log('Done.');
