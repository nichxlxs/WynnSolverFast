'use strict';

const { annotatePruningResults, median } = require('./pruning_metrics');

function descendingRanks(rows, valueKey, rankKey) {
    const ordered = [...rows].sort((left, right) => {
        const leftValue = Number.isFinite(left[valueKey]) ? left[valueKey] : -Infinity;
        const rightValue = Number.isFinite(right[valueKey]) ? right[valueKey] : -Infinity;
        return rightValue - leftValue;
    });
    let previous = null;
    let rank = 0;
    for (let index = 0; index < ordered.length; index++) {
        const value = ordered[index][valueKey];
        if (index === 0 || value !== previous) rank = index + 1;
        ordered[index][rankKey] = rank;
        previous = value;
    }
}

function checkpointAt(result, checkpointMs) {
    return (result.checkpoints || []).find(checkpoint => checkpoint.checkpoint_ms === checkpointMs) ?? null;
}

function paretoFlag(rows, row) {
    const metrics = ['combination_reduction', 'checkpoint_checked_per_second', 'checkpoint_score_ratio'];
    return !rows.some(other => {
        if (other === row) return false;
        const neverWorse = metrics.every(metric => (other[metric] ?? -Infinity) >= (row[metric] ?? -Infinity));
        const strictlyBetter = metrics.some(metric => (other[metric] ?? -Infinity) > (row[metric] ?? -Infinity));
        return neverWorse && strictlyBetter;
    });
}

function analyzeAnytimeCampaign(results, checkpointSeconds) {
    const checkpointMs = checkpointSeconds * 1000;
    const measured = annotatePruningResults(results);
    const groups = new Map();
    for (const result of measured) {
        const key = `${result.profile_id}\0${result.variant}`;
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(result);
    }

    const rows = [];
    for (const group of groups.values()) {
        const checkpoints = group.map(result => ({ result, checkpoint: checkpointAt(result, checkpointMs) }));
        const bestScore = Math.max(...checkpoints
            .map(entry => entry.checkpoint?.best_score)
            .filter(Number.isFinite));
        const ranked = checkpoints.map(({ result, checkpoint }) => ({
            profile_id: result.profile_id,
            class_name: result.class_name,
            build_family: result.build_family,
            variant: result.variant,
            pruning_mode: result.pruning_mode,
            checkpoint_seconds: checkpointSeconds,
            input_combinations: result.input_combinations,
            search_combinations: result.search_combinations,
            combinations_removed: result.combinations_removed,
            combination_reduction: result.combination_reduction,
            checkpoint_checked: checkpoint?.checked ?? null,
            checkpoint_checked_per_second: checkpoint?.checked_per_second ?? null,
            checkpoint_observed_score: checkpoint?.best_score ?? null,
            checkpoint_time_to_best_seconds: Number.isFinite(checkpoint?.best_score_observed_at_ms)
                ? checkpoint.best_score_observed_at_ms / 1000 : null,
            checkpoint_score_ratio: Number.isFinite(checkpoint?.best_score) && Number.isFinite(bestScore)
                && bestScore !== 0 ? checkpoint.best_score / bestScore : null,
            checkpoint_best_across_strategies: Number.isFinite(bestScore) ? bestScore : null,
            target_score: result.target_score,
            target_recovered_at_checkpoint: Number.isFinite(checkpoint?.best_score)
                && Number.isFinite(result.target_score)
                ? checkpoint.best_score >= result.target_score : null,
            completed: result.completed,
        }));
        descendingRanks(ranked, 'combination_reduction', 'reduction_rank');
        descendingRanks(ranked, 'checkpoint_checked_per_second', 'throughput_rank');
        descendingRanks(ranked, 'checkpoint_score_ratio', 'score_rank');
        for (const row of ranked) {
            row.pareto = paretoFlag(ranked, row);
            rows.push(row);
        }
    }

    const summaryGroups = new Map();
    for (const row of rows) {
        const key = `${row.variant}\0${row.pruning_mode}`;
        if (!summaryGroups.has(key)) summaryGroups.set(key, []);
        summaryGroups.get(key).push(row);
    }
    const summaries = [...summaryGroups.values()].map(group => ({
        variant: group[0].variant,
        pruning_mode: group[0].pruning_mode,
        checkpoint_seconds: checkpointSeconds,
        scenario_count: group.length,
        total_input_combinations: group.reduce((sum, row) => sum + row.input_combinations, 0),
        total_search_combinations: group.reduce((sum, row) => sum + row.search_combinations, 0),
        total_combinations_removed: group.reduce((sum, row) => sum + row.combinations_removed, 0),
        aggregate_combination_reduction: (() => {
            const input = group.reduce((sum, row) => sum + row.input_combinations, 0);
            const removed = group.reduce((sum, row) => sum + row.combinations_removed, 0);
            return input ? removed / input : 0;
        })(),
        median_combination_reduction: median(group.map(row => row.combination_reduction)),
        median_checked_per_second: median(group.map(row => row.checkpoint_checked_per_second).filter(Number.isFinite)),
        median_score_ratio: median(group.map(row => row.checkpoint_score_ratio).filter(Number.isFinite)),
        worst_score_ratio: (() => {
            const ratios = group.map(row => row.checkpoint_score_ratio).filter(Number.isFinite);
            return ratios.length ? Math.min(...ratios) : null;
        })(),
        target_recovered_count: group.filter(row => row.target_recovered_at_checkpoint === true).length,
        mean_reduction_rank: group.reduce((sum, row) => sum + row.reduction_rank, 0) / group.length,
        mean_throughput_rank: group.reduce((sum, row) => sum + row.throughput_rank, 0) / group.length,
        mean_score_rank: group.reduce((sum, row) => sum + row.score_rank, 0) / group.length,
        reduction_wins: group.filter(row => row.reduction_rank === 1).length,
        throughput_wins: group.filter(row => row.throughput_rank === 1).length,
        score_wins: group.filter(row => row.score_rank === 1).length,
        pareto_count: group.filter(row => row.pareto).length,
    }));
    return { checkpoint_seconds: checkpointSeconds, summaries, rows };
}

module.exports = { analyzeAnytimeCampaign, checkpointAt, descendingRanks };
