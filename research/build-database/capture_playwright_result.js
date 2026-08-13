#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');

let input = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', chunk => { input += chunk; });
process.stdin.on('end', () => {
    const match = input.match(/### Result\r?\n([\s\S]*?)\r?\n### Ran Playwright code/);
    if (!match) {
        process.stderr.write(input);
        throw new Error('Could not find the Playwright CLI result payload');
    }
    const payload = JSON.parse(match[1]);
    const outputPath = path.join(__dirname, process.argv[2] || 'nori-browser-validation.json');
    fs.writeFileSync(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
    const results = payload.results || [];
    const summary = {
        source_record_count: payload.source_record_count,
        record_count: results.length,
        valid_current_builder: results.filter(result => result.valid_current_builder).length,
        broken_current_builder: results.filter(result => result.status === 'broken_current_builder').length,
        not_wynnbuilder_link: results.filter(result => result.status === 'not_wynnbuilder_link').length,
        ability_tree_valid: results.filter(result => result.ability_tree_valid).length,
        version_update_prompted: results.filter(result => result.version_update_prompted).length,
        weapon_metadata_mismatches: results.filter(result =>
            result.loaded_weapon && !result.weapon_metadata_matches,
        ).length,
    };
    console.log(JSON.stringify(summary, null, 2));
});
