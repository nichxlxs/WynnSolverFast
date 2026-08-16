'use strict';

const SCORE_RELATIVE_TOLERANCE = 1e-9;

function median(values) {
    if (!values.length) return null;
    const ordered = [...values].sort((left, right) => left - right);
    const middle = Math.floor(ordered.length / 2);
    return ordered.length % 2
        ? ordered[middle]
        : (ordered[middle - 1] + ordered[middle]) / 2;
}

function scenarioKey(result) {
    return `${result.profile_id}\0${result.variant}`;
}

function scoreMatches(left, right) {
    if (!Number.isFinite(left) || !Number.isFinite(right)) return false;
    const tolerance = Math.max(1e-6, Math.abs(right) * SCORE_RELATIVE_TOLERANCE);
    return Math.abs(left - right) <= tolerance;
}

function annotatePruningResults(results) {
    const measured = results.map(result => {
        const input = result.input_combinations ?? 0;
        const searched = result.search_combinations ?? 0;
        const elapsedSeconds = (result.elapsed_ms ?? 0) / 1000;
        const completed = !result.timed_out;
        return {
            ...result,
            completed,
            combinations_removed: Math.max(0, input - searched),
            combination_reduction: input > 0 ? Math.max(0, 1 - searched / input) : 0,
            checked_per_second: elapsedSeconds > 0 ? (result.checked ?? 0) / elapsedSeconds : null,
            search_space_max_score: completed ? result.best_score : null,
            maximum_score_scope: completed
                ? (result.pruning_mode === 'off' ? 'full_input_space' : 'pruned_search_space')
                : 'censored_timeout',
        };
    });

    const fullSpaceOptima = new Map();
    for (const result of measured) {
        if (result.pruning_mode === 'off' && result.completed) {
            fullSpaceOptima.set(scenarioKey(result), result.best_score);
        }
    }

    return measured.map(result => {
        const fullSpaceMax = fullSpaceOptima.get(scenarioKey(result));
        if (!Number.isFinite(fullSpaceMax)) {
            return {
                ...result,
                full_space_max_score: null,
                global_optimum_preserved: null,
                optimality_status: 'unknown_no_exhaustive_control',
            };
        }
        if (result.best_score > fullSpaceMax && !scoreMatches(result.best_score, fullSpaceMax)) {
            throw new Error(`${result.snapshot}/${result.pruning_mode}: score exceeds exhaustive control`);
        }
        if (result.pruning_mode === 'off') {
            return {
                ...result,
                full_space_max_score: fullSpaceMax,
                global_optimum_preserved: true,
                optimality_status: 'exhaustive_control',
            };
        }
        if (scoreMatches(result.best_score, fullSpaceMax)) {
            return {
                ...result,
                full_space_max_score: fullSpaceMax,
                global_optimum_preserved: true,
                optimality_status: 'global_optimum_found',
            };
        }
        if (result.completed) {
            return {
                ...result,
                full_space_max_score: fullSpaceMax,
                global_optimum_preserved: false,
                optimality_status: 'global_optimum_pruned',
            };
        }
        return {
            ...result,
            full_space_max_score: fullSpaceMax,
            global_optimum_preserved: null,
            optimality_status: 'inconclusive_pruned_search_timed_out',
        };
    });
}

function summarizePruningResults(results) {
    const completed = results.filter(result => result.completed ?? !result.timed_out);
    const recovered = results.filter(result => result.recovered);
    const recoveryTimes = recovered
        .map(result => result.time_to_recovery_ms)
        .filter(Number.isFinite);
    const totalInput = results.reduce((total, result) => total + (result.input_combinations ?? 0), 0);
    const totalSearch = results.reduce((total, result) => total + (result.search_combinations ?? 0), 0);
    const totalRemoved = results.reduce((total, result) => total + (result.combinations_removed
        ?? Math.max(0, (result.input_combinations ?? 0) - (result.search_combinations ?? 0))), 0);
    const totalElapsedMs = results.reduce((total, result) => total + (result.elapsed_ms ?? 0), 0);
    const totalChecked = results.reduce((total, result) => total + (result.checked ?? 0), 0);
    const throughputs = results.map(result => result.checked_per_second
        ?? ((result.elapsed_ms ?? 0) > 0 ? (result.checked ?? 0) / (result.elapsed_ms / 1000) : null))
        .filter(Number.isFinite);
    return {
        scenario_count: results.length,
        completed_count: completed.length,
        recovered_count: recovered.length,
        missed_count: results.length - recovered.length,
        recovery_rate: results.length ? recovered.length / results.length : null,
        max_regret: results.length ? Math.max(...results.map(entry => entry.regret ?? 0)) : null,
        total_elapsed_ms: totalElapsedMs,
        median_elapsed_ms: median(results.map(entry => entry.elapsed_ms).filter(Number.isFinite)),
        median_time_to_recovery_ms: median(recoveryTimes),
        median_regret: median(results.map(entry => entry.regret).filter(Number.isFinite)),
        total_input_combinations: totalInput,
        total_search_combinations: totalSearch,
        total_combinations_removed: totalRemoved,
        aggregate_combination_reduction: totalInput > 0 ? totalRemoved / totalInput : null,
        median_combination_reduction: median(results.map(result => result.combination_reduction
            ?? ((result.input_combinations ?? 0) > 0
                ? 1 - result.search_combinations / result.input_combinations : 0))),
        total_checked: totalChecked,
        aggregate_checked_per_second: totalElapsedMs > 0 ? totalChecked / (totalElapsedMs / 1000) : null,
        median_checked_per_second: median(throughputs),
        exhaustive_full_space_count: results.filter(result => result.maximum_score_scope === 'full_input_space').length,
        exhaustive_pruned_space_count: results.filter(result => result.maximum_score_scope === 'pruned_search_space').length,
        optimum_preserved_count: results.filter(result => result.global_optimum_preserved === true).length,
        optimum_pruned_count: results.filter(result => result.global_optimum_preserved === false).length,
    };
}

module.exports = {
    annotatePruningResults,
    median,
    scoreMatches,
    summarizePruningResults,
};
