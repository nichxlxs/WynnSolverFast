#!/usr/bin/env python3
"""Scored wide-pool regression using a known-optimum additive HP model.

Derive only the runtime/schema constants from an exported score fixture;
replace equipment, restrictions, effects and objective with synthetic data.
Each case exhausts a small Cartesian space and checks its analytic top 15
against bounds off, the historical >=128 bypass, and the wide-key path.

Usage: python test_wide_bound_keys.py --score-base fixtures/score_spell2.json
"""
import argparse
import copy
import itertools
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time


def make_case(base, sizes, directory):
    score = copy.deepcopy(base)
    score['meta'] = {'scoring_target': 'total_hp', 'cases': 0,
                     'note': 'synthetic wide-key correctness test, not a game workload'}
    score['cases'] = []
    score['parsed_combo'] = []
    score['boost_registry'] = []
    score['atree_hit_refs'] = {}
    layer = score['layer2']
    layer.update(restrictions={'stat_thresholds': []}, static_boosts=[],
                 radiance_boost=False, sets_data={}, tome_sms=[],
                 guild_tome_sm=None, tome_opt=False, guild_tome_candidates=[],
                 tome_wa_bundles=[], tome_bound=None, hp_casting=False,
                 health_config=None, scaling_plan={'kind': 'cached', 'scaled': {'__m': {}}})
    layer['constants']['hp_base_for_level'] = 610
    layer['item_registry'] = {}
    weapon = score['weapon_sm']['__m']
    weapon['reqs'] = [0] * 5
    weapon['skillpoints'] = [0] * 5
    weapon['hp'] = 0
    weapon.pop('set', None)
    for stat in ('str', 'dex', 'int', 'def', 'agi'):
        weapon[stat] = 0
        weapon[stat + 'Req'] = 0
    for rolls in ('maxRolls', 'minRolls'):
        weapon[rolls]['__m'] = {k: 0 for k in weapon[rolls]['__m']}

    lines = ['BUDGET 200', 'PRECHECKS 0', 'EHP 0 0 0 0',
             'EHPNA 0 0 0 0', 'THP 0 0 0', 'HPSTART 0',
             'WEAPON ' + ' '.join(['0'] * 10), 'GUILD 0',
             'NFIXED 0', f'NSLOTS {len(sizes)}']
    pools = []
    hp_pools = []
    slot_types = ['helmet', 'chestplate', 'leggings']
    for depth, size in enumerate(sizes):
        names = []
        hps = []
        lines.append(f'SLOT {slot_types[depth]} {depth} 0 0 {size}')
        for i in range(size):
            name = f'WideKey{depth}_{i}'
            # Place the optimum at the highest offset (including >128 and
            # >255). Unique mixed-radix weights give an exact top-15 oracle.
            hp = (i + 1) * math.prod(sizes[depth + 1:])
            item = copy.deepcopy(layer['none_item_sms'][depth])
            item['__m'].update(name=name, displayName=name, hp=hp, id=20000 + depth * 10000 + i)
            layer['item_registry'][name] = item
            names.append(name)
            hps.append(hp)
            lines.append('ITEM 0 ' + ' '.join(['0'] * 10) + f' -1 -1 {hp}')
        pools.append(names)
        hp_pools.append(hps)
    lines.extend(['NSETS 0', 'NAMES 1'])
    for depth, names in enumerate(pools):
        lines.append(f'INAMES {depth} {len(names)}')
        lines.extend(names)
    lines.extend(['FNAMES 0', 'NONENAMES 8'])
    lines.extend(item['__m']['name'] for item in layer['none_item_sms'])
    enum_path = directory / 'enum.txt'
    score_path = directory / 'score.json'
    enum_path.write_text('\n'.join(lines) + '\n')
    score_path.write_text(json.dumps(score))
    expected = sorted((610 + sum(x) for x in itertools.product(*hp_pools)), reverse=True)[:15]
    return enum_path, score_path, expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--score-base', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path(__file__).resolve().parent / 'target/release/enum_kernel')
    parser.add_argument('--json', type=Path)
    args = parser.parse_args()
    base = json.loads(args.score_base.read_text())
    modes = {
        'bounds_off': {'BOUND_DEPTH': '0', 'BOUND_TAIL': '0', 'BOUND_CLUSTER': '0'},
        'legacy': {'WIDE_BOUND_KEYS': '0'},
        'wide': {'WIDE_BOUND_KEYS': '1'},
    }
    rows = []
    for sizes in [(127, 3, 5), (128, 3, 5), (129, 3, 5),
                  (257, 3, 5), (3, 257, 5), (3, 5, 257), (2050, 3, 5)]:
        with tempfile.TemporaryDirectory(prefix='wynn-wide-keys-') as td:
            enum_path, score_path, expected = make_case(base, sizes, Path(td))
            for mode, overrides in modes.items():
                env = dict(os.environ)
                for key in ['ENUM_LEAF_BUDGET', 'ENUM_TIME_CAP_SECS', 'SCORE_TRACE']:
                    env.pop(key, None)
                env.update(WARM_K='0', BOUND_DEPTH='3', BOUND_TAIL='1',
                           BOUND_CLUSTER='4', SUPER_CLUSTER='1', SCORE_DENSE='1',
                           SP_BOUND_OFF='1', WIDE_BOUND_KEYS='1')
                env.update(overrides)
                start = time.perf_counter()
                proc = subprocess.run([str(args.binary.resolve()), str(enum_path), '1', str(score_path)],
                                      env=env, text=True, capture_output=True, timeout=90, check=True)
                elapsed = time.perf_counter() - start
                output = proc.stdout + proc.stderr
                scores = [float(v) for v in re.findall(r'^top15: ([^ ]+) \|', output, re.M)]
                if scores != expected:
                    raise AssertionError(f'{sizes}/{mode}: actual {scores}, expected {expected}\n{output}')
                checked = int(re.search(r'enum_kernel: checked (\d+)', output)[1])
                assert checked == math.prod(sizes), (sizes, mode, checked)
                pruned = int(re.search(r'bound_pruned (\d+)', output)[1])
                row = dict(sizes=sizes, mode=mode, checked=checked,
                           bound_pruned=pruned, elapsed_seconds=elapsed, scores=scores)
                rows.append(row)
                print(f'{sizes!s:16} {mode:10} exact top-15; {checked:6} checked; {pruned:6} bound-pruned; {elapsed:.4f}s')
    assert any(r['bound_pruned'] > 0 and max(r['sizes']) >= 128 and r['mode'] == 'wide' for r in rows)
    if args.json:
        args.json.write_text(json.dumps(rows, indent=2) + '\n')
    print(f'{len(rows)} exhaustive synthetic scored regressions passed')


if __name__ == '__main__':
    main()
