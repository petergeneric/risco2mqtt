'use strict';

const path = require('path');
const { execFileSync } = require('child_process');

const node = process.execPath;

function run(label, script) {
    console.log(`\n=== ${label} ===\n`);
    try {
        execFileSync(node, [path.join(__dirname, script)], {
            stdio: 'inherit',
            cwd: path.join(__dirname, '..'),
        });
        return true;
    } catch (e) {
        return false;
    }
}

let ok = true;

// Step 1: Generate vectors
if (!run('Generating test vectors', 'generate-vectors.js')) {
    console.error('\nVector generation failed!');
    process.exit(1);
}

// Step 2: Run tests
if (!run('Crypto compatibility tests', 'crypto.test.js')) {
    ok = false;
}

if (ok) {
    console.log('\nAll tests passed.');
    process.exit(0);
} else {
    console.error('\nSome tests failed.');
    process.exit(1);
}
