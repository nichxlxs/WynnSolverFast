#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');

const version = process.argv[2] || '2.2.3.0';
const files = [
    'items.json',
    'tomes.json',
    'atree.json',
    'aspects.json',
    'encoding_consts.json',
    'majid.json',
    'dps_data.json',
    'recipes.json',
    'ingreds.json',
];
const root = path.resolve(__dirname, '../..');
const outputDir = path.join(root, 'data', version);

async function main() {
    fs.mkdirSync(outputDir, { recursive: true });
    const written = [];
    for (const filename of files) {
        const sourceUrl = `https://wynnbuilder.github.io/data/${version}/${filename}`;
        const response = await fetch(sourceUrl, {
            headers: { 'User-Agent': 'WynnSolverFast current-data importer' },
        });
        if (!response.ok) throw new Error(`${sourceUrl} returned HTTP ${response.status}`);
        const text = await response.text();
        JSON.parse(text);
        fs.writeFileSync(path.join(outputDir, filename), text.endsWith('\n') ? text : `${text}\n`, 'utf8');
        written.push({ filename, bytes: Buffer.byteLength(text), source_url: sourceUrl });
    }
    console.log(JSON.stringify({ version, output_directory: outputDir, files: written }, null, 2));
}

main().catch(error => {
    console.error(error.stack || error.message);
    process.exitCode = 1;
});
