#!/usr/bin/env node
'use strict';

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const endpoint = 'https://nori.fish/api/build/search';
const catalogueUrl = 'https://nori.fish/wynn/build/';
const classes = ['warrior', 'mage', 'archer', 'assassin', 'shaman'];
const outputPath = path.join(__dirname, 'nori-builds.json');

function slug(value) {
    return String(value || 'build')
        .normalize('NFKD')
        .replace(/[^a-zA-Z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '')
        .toLowerCase()
        .slice(0, 48) || 'build';
}

function sha256(value) {
    return crypto.createHash('sha256').update(value).digest('hex');
}

function parseBuilderUrl(rawLink) {
    const url = new URL(rawLink);
    const encoded = url.hash.startsWith('#') ? url.hash.slice(1) : url.hash;
    const isLegacyBuilder = url.searchParams.has('v') || /^9_/.test(encoded);
    const isNoriSolverPreset = url.host.toLowerCase() === 'rawfish69.github.io'
        && url.pathname.startsWith('/build-solver/');
    const separator = !isLegacyBuilder && !isNoriSolverPreset ? encoded.indexOf('_') : -1;
    const buildHash = isNoriSolverPreset
        ? url.searchParams.get('wb') || ''
        : separator >= 0 ? encoded.slice(0, separator) : encoded;
    const solverHash = separator >= 0 ? encoded.slice(separator + 1) : null;
    return {
        url: url.toString(),
        host: url.host.toLowerCase(),
        path: url.pathname,
        format: isNoriSolverPreset ? 'nori_solver_preset' : isLegacyBuilder ? 'legacy_builder' : 'compact_builder',
        declared_version: url.searchParams.get('v'),
        build_hash: buildHash,
        solver_hash: solverHash,
        build_hash_sha256: sha256(buildHash),
    };
}

async function main() {
    const response = await fetch(endpoint, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
            'User-Agent': 'WynnSolverFast research catalogue importer',
        },
        body: JSON.stringify({ keyword: '', class_types: classes }),
    });
    if (!response.ok) throw new Error(`Nori API returned HTTP ${response.status}`);
    const sourceRecords = await response.json();
    if (!Array.isArray(sourceRecords)) throw new Error('Nori API response was not an array');

    const builds = sourceRecords.map((record, sourceIndex) => {
        let builder;
        let parseError = null;
        try {
            builder = parseBuilderUrl(record.link);
        } catch (error) {
            parseError = error.message;
            builder = null;
        }
        const identity = builder?.build_hash || `${record.class}|${record.weapon}|${record.name}|${sourceIndex}`;
        return {
            id: `nori-${slug(record.class)}-${slug(record.weapon)}-${sha256(identity).slice(0, 12)}`,
            source_index: sourceIndex,
            name: record.name || null,
            class: record.class || null,
            weapon: record.weapon || null,
            tags_raw: record.tag || null,
            tags: String(record.tag || '').split(',').map(value => value.trim()).filter(Boolean),
            icon_url: record.icon || null,
            credit: record.credit || null,
            source_url: catalogueUrl,
            source_builder_url: record.link || null,
            builder,
            parse_error: parseError,
        };
    });

    const byHash = new Map();
    const byExactUrl = new Map();
    for (const build of builds) {
        const hash = build.builder?.build_hash;
        if (hash) {
            if (!byHash.has(hash)) byHash.set(hash, []);
            byHash.get(hash).push(build.id);
        }
        if (build.source_builder_url) {
            if (!byExactUrl.has(build.source_builder_url)) byExactUrl.set(build.source_builder_url, []);
            byExactUrl.get(build.source_builder_url).push(build.id);
        }
    }
    const duplicateHashGroups = [...byHash.entries()]
        .filter(([, ids]) => ids.length > 1)
        .map(([buildHash, ids]) => ({ build_hash: buildHash, build_ids: ids }));
    const duplicateUrlGroups = [...byExactUrl.entries()]
        .filter(([, ids]) => ids.length > 1)
        .map(([url, ids]) => ({ url, build_ids: ids }));

    const countsByClass = Object.fromEntries(
        [...new Set(builds.map(build => build.class))]
            .sort()
            .map(playerClass => [playerClass, builds.filter(build => build.class === playerClass).length]),
    );
    const countsByHost = Object.fromEntries(
        [...new Set(builds.map(build => build.builder?.host || 'invalid'))]
            .sort()
            .map(host => [host, builds.filter(build => (build.builder?.host || 'invalid') === host).length]),
    );

    const output = {
        schema_version: 1,
        fetched_at: new Date().toISOString(),
        source: {
            catalogue_url: catalogueUrl,
            api_endpoint: endpoint,
            request: { keyword: '', class_types: classes },
            pagination: 'The Nori client fetches the complete filtered result and applies ten-record pages locally.',
        },
        summary: {
            source_record_count: sourceRecords.length,
            normalized_record_count: builds.length,
            parse_error_count: builds.filter(build => build.parse_error).length,
            unique_build_hash_count: byHash.size,
            unique_exact_url_count: byExactUrl.size,
            duplicate_build_hash_group_count: duplicateHashGroups.length,
            duplicate_exact_url_group_count: duplicateUrlGroups.length,
            counts_by_class: countsByClass,
            counts_by_host: countsByHost,
        },
        duplicate_build_hash_groups: duplicateHashGroups,
        duplicate_exact_url_groups: duplicateUrlGroups,
        builds,
    };
    fs.writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
    console.log(JSON.stringify(output.summary, null, 2));
}

main().catch(error => {
    console.error(error.stack || error.message);
    process.exitCode = 1;
});
