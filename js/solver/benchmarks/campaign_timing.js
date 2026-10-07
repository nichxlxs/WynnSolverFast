'use strict';

const DEFAULT_TIMING_GRACE_MS = 10000;

function assessCampaignTiming(result, graceMs = DEFAULT_TIMING_GRACE_MS) {
    const violations = [];
    const capMs = Number(result.campaign_seconds) * 1000;
    if (Number.isFinite(capMs) && Number(result.elapsed_ms) > capMs + graceMs) {
        violations.push(`elapsed ${result.elapsed_ms}ms exceeds ${capMs}ms cap plus ${graceMs}ms grace`);
    }
    for (const checkpoint of result.checkpoints || []) {
        if (checkpoint.search_completed_before_checkpoint) continue;
        if (Number(checkpoint.observed_at_ms) > Number(checkpoint.checkpoint_ms) + graceMs) {
            violations.push(`checkpoint ${checkpoint.checkpoint_ms}ms observed at ${checkpoint.observed_at_ms}ms`);
        }
    }
    return { valid: violations.length === 0, grace_ms: graceMs, violations };
}

module.exports = { DEFAULT_TIMING_GRACE_MS, assessCampaignTiming };
