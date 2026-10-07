// ── Contract-aware candidate reduction ──────────────────────────────────────
//
// Deep module interface:
//   reduce_candidate_pools(pools, context)
//
// The reducer owns pruning policy, contract guards, active/deferred pool
// construction, and reduction diagnostics. Search callers consume only the
// returned active pools and do not need to understand dominance internals.
//
// Loaded after item_priority.js and before search.js.

const CANDIDATE_REDUCTION_POLICIES = Object.freeze({
    off: Object.freeze({ enabled: false, sensitivity_ratio: 0.005, guard: 'none', exact: true }),
    certified: Object.freeze({ enabled: true, sensitivity_ratio: 0, guard: 'certified', exact: true }),
    balanced: Object.freeze({ enabled: true, sensitivity_ratio: 0.005, guard: 'structural', exact: false }),
    conservative: Object.freeze({ enabled: true, sensitivity_ratio: 0, guard: 'legacy', exact: false }),
    current: Object.freeze({ enabled: true, sensitivity_ratio: 0.005, guard: 'legacy', exact: false }),
    aggressive: Object.freeze({ enabled: true, sensitivity_ratio: 0.02, guard: 'legacy', exact: false }),
});

function get_candidate_reduction_policy(mode) {
    const name = mode ?? 'balanced';
    const policy = CANDIDATE_REDUCTION_POLICIES[name];
    if (!policy) {
        throw new Error(`unknown candidate reduction mode ${name}; expected ${Object.keys(CANDIDATE_REDUCTION_POLICIES).join(', ')}`);
    }
    return { name, ...policy };
}

function get_candidate_search_stages(mode) {
    if (mode === 'fast_verify') return ['balanced', 'off'];
    get_candidate_reduction_policy(mode);
    return [mode ?? 'balanced'];
}

function _candidate_clone_pools(pools) {
    return Object.fromEntries(Object.entries(pools).map(([slot, pool]) => [slot, [...pool]]));
}

function _candidate_has_melee(snap) {
    return (snap.parsed_combo ?? []).some(row => {
        if (!row.spell) return false;
        const base_spell = row.spell.mana_derived_from ?? row.spell.base_spell;
        return base_spell === 0 || row.spell.scaling === 'melee' || row.is_melee_time;
    });
}

function _candidate_require_equal(dominance_stats, stat) {
    dominance_stats.higher.delete(stat);
    dominance_stats.lower.delete(stat);
    dominance_stats.equal.add(stat);
}

function _candidate_apply_contract_guards(dominance_stats, snap, dmg_weights, policy) {
    if (policy.guard !== 'structural' && policy.guard !== 'certified') return;

    // A nonzero objective response below the heuristic cutoff is uncertain,
    // not irrelevant. Keep it as an equality bucket so a cheaper-requirement
    // item cannot erase a weak but useful damage/sustain dimension.
    if (policy.guard === 'structural') {
        for (const [stat, weight] of dmg_weights) {
            if (!weight
                || dominance_stats.higher.has(stat)
                || dominance_stats.lower.has(stat)
                || dominance_stats.equal.has(stat)) continue;
            dominance_stats.equal.add(stat);
        }
    }

    // Certified reduction proves replacement under every directly modelled
    // item dimension. A zero local derivative means "not observed at this
    // baseline", not "globally irrelevant", so omitted dimensions must
    // match exactly. Powder-slot capacity is structural rather than an ID and
    // receives the same treatment.
    if (policy.guard === 'certified') {
        for (const stat of [..._PERTURBABLE_STATS, 'hp', 'slots']) {
            if (dominance_stats.higher.has(stat)
                || dominance_stats.lower.has(stat)
                || dominance_stats.equal.has(stat)) continue;
            dominance_stats.equal.add(stat);
        }
    }

    // Attack tier is discrete and can change hit cadence, per-hit damage,
    // mana/life-steal cadence, and ability-state transitions. A local
    // first-order probe can legitimately report zero at a tier cap or
    // degenerate baseline, so melee contracts compare it only within an
    // equal-value bucket.
    if (_candidate_has_melee(snap)) {
        _candidate_require_equal(dominance_stats, 'atkTier');
    }
}

function _candidate_deferred_pools(original, active) {
    const deferred = {};
    for (const [slot, pool] of Object.entries(original)) {
        const survivors = new Set(active[slot] ?? []);
        deferred[slot] = pool.filter(item => !survivors.has(item));
    }
    return deferred;
}

function _candidate_pool_counts(pools) {
    return Object.fromEntries(Object.entries(pools).map(([slot, pool]) => [slot, pool.length]));
}

function reduce_candidate_pools(pools, context = {}) {
    const policy = get_candidate_reduction_policy(context.mode);
    const original_pools = _candidate_clone_pools(pools);
    const active_pools = _candidate_clone_pools(pools);
    const input_counts = _candidate_pool_counts(original_pools);

    const snap = context.snap ?? {};
    const restrictions = context.restrictions ?? { stat_thresholds: [] };
    const dmg_weights = context.dmg_weights ?? new Map();
    const dominance_stats = _build_dominance_stats(
        snap,
        dmg_weights,
        restrictions,
        { sensitivity_ratio: policy.sensitivity_ratio },
    );

    // Unpruned search still exposes the contract dimensions. Tome
    // optimisation and diagnostics consume them even though equipment
    // candidates are not removed.
    if (!policy.enabled) {
        return {
            mode: policy.name,
            policy,
            active_pools,
            deferred_pools: Object.fromEntries(Object.keys(active_pools).map(slot => [slot, []])),
            dominance_stats,
            removed_count: 0,
            input_counts,
            active_counts: { ...input_counts },
            certificates: [],
        };
    }

    _candidate_apply_contract_guards(dominance_stats, snap, dmg_weights, policy);

    const pruning_report = _prune_dominated_items(active_pools, dominance_stats, {
        preserve_set_items: context.preserve_set_items !== false,
        return_report: true,
    });
    const deferred_pools = _candidate_deferred_pools(original_pools, active_pools);

    return {
        mode: policy.name,
        policy,
        active_pools,
        deferred_pools,
        dominance_stats,
        removed_count: pruning_report.removed_count,
        input_counts,
        active_counts: _candidate_pool_counts(active_pools),
        certificates: pruning_report.certificates,
    };
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        CANDIDATE_REDUCTION_POLICIES,
        get_candidate_reduction_policy,
        get_candidate_search_stages,
    };
}
