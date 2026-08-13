/*
 * Non exhaustive list of dependencies (add them here if you see them!)
 *
 * js/game/build_utils.js:skp_order
 * js/data/load_item.js:sets
 * js/game/game_rules.js:SP_PER_ATTR_CAP
 */


/**
 * Calculate equipment required skillpoints using pruned activation-order search.
 *
 * Under cascade mechanics, an item's SP bonus only activates once ALL of its
 * requirements are met by assigned SP + bonuses from already-activated items.
 * A used-item bit mask and lower bounds prune the permutation search for the
 * activation ordering that minimizes total assigned SP.
 *
 * @param {Map[]} equipment  - equipment statMaps (armor/acc/tomes)
 * @param {Map}   weapon     - weapon statMap
 * @param {number} sp_budget - max total assignable SP (default Infinity = no limit)
 * @param {Map|null} scratch_set_counts - reusable Map for set counting (optional)
 * @param {Object|null} scratch_sp - reusable arrays to eliminate per-call allocations (optional).
 *                                   Caller must .slice() return arrays before caching.
 * @returns {Array|null} [best_skillpoints, final_skillpoints, best_total, set_counts, total_item_skillpoints],
 *                       or null if sp_budget is exceeded or any single attr > SP_PER_ATTR_CAP.
 *                       total_item_skillpoints is the per-attr sum of SP granted by items + set
 *                       bonuses (used by Radiance to scale item-granted SP).
 */
function calculate_skillpoints(equipment, weapon, sp_budget = Infinity, scratch_set_counts = null, scratch_sp = null) {
    let no_bonus_items;
    let assign;
    let final_skillpoints;
    let free_bonus;
    let max_passive_req;
    let ord_items;
    let ord_reqs;
    let ord_skp;
    let post_floor;
    let running_bonus;
    let best_assign;
    let save_stack;
    let total_item_skillpoints;

    if (scratch_sp) {
        no_bonus_items = scratch_sp.no_bonus;
        no_bonus_items[0] = weapon;
        scratch_sp._no_bonus_len = 1;
        assign = scratch_sp.assign;
        final_skillpoints = scratch_sp.final;
        free_bonus = scratch_sp.free_bonus;
        max_passive_req = scratch_sp.max_passive_req;
        ord_items = scratch_sp.ord_items;
        ord_reqs = scratch_sp.ord_reqs;
        ord_skp = scratch_sp.ord_skp;
        post_floor = scratch_sp.post_floor;
        running_bonus = scratch_sp.running_bonus;
        best_assign = scratch_sp.best_assign;
        save_stack = scratch_sp.save_stack;
        total_item_skillpoints = scratch_sp.total_item_skp;
        for (let i = 0; i < 5; i++) {
            assign[i] = 0;
            free_bonus[i] = 0;
            max_passive_req[i] = 0;
            total_item_skillpoints[i] = 0;
        }
    } else {
        no_bonus_items = [weapon];
        assign = [0, 0, 0, 0, 0];
        free_bonus = [0, 0, 0, 0, 0];
        max_passive_req = [0, 0, 0, 0, 0];
        ord_items = new Array(9);
        ord_reqs = new Array(9);
        ord_skp = new Array(9);
        post_floor = [0, 0, 0, 0, 0];
        running_bonus = [0, 0, 0, 0, 0];
        best_assign = [0, 0, 0, 0, 0];
        save_stack = new Array(45);
        total_item_skillpoints = [0, 0, 0, 0, 0];
    }

    // ── Phase 1: Classify items ─────────────────────────────────────────────

    let set_counts;
    if (scratch_set_counts) {
        set_counts = scratch_set_counts;
        set_counts.clear();
    } else {
        set_counts = new Map();
    }

    let k = 0; // number of ordering items

    for (const item of equipment) {
        const is_crafted = item.get('crafted');
        const req = item.get('reqs');
        const skp = item.get('skillpoints');

        for (let i = 0; i < 5; i++) total_item_skillpoints[i] += skp[i];

        // Track set membership (non-crafted only)
        if (!is_crafted) {
            const set_name = item.get('set');
            if (set_name) {
                if (!set_counts.get(set_name)) {
                    set_counts.set(set_name, 0);
                }
                set_counts.set(set_name, set_counts.get(set_name) + 1);
            }
        }

        if (is_crafted) {
            // Crafted items: always passive, SP added unconditionally at end
            if (scratch_sp) {
                no_bonus_items[scratch_sp._no_bonus_len++] = item;
            } else {
                no_bonus_items.push(item);
            }
            // Track passive requirements
            for (let i = 0; i < 5; i++) {
                if (req[i] > max_passive_req[i]) max_passive_req[i] = req[i];
            }
        } else {
            let has_req = false;
            let has_skp = false;
            for (let i = 0; i < 5; i++) {
                if (req[i] > 0) has_req = true;
                if (skp[i] !== 0) has_skp = true;
            }

            if (has_req && has_skp) {
                // Ordering item: DP candidate
                ord_items[k] = item;
                ord_reqs[k] = req;
                ord_skp[k] = skp;
                k++;
            } else if (!has_req) {
                // Free item: no requirements, add SP to free pool immediately
                for (let i = 0; i < 5; i++) free_bonus[i] += skp[i];
            } else {
                // Passive item: has reqs but no SP bonus
                for (let i = 0; i < 5; i++) {
                    if (req[i] > max_passive_req[i]) max_passive_req[i] = req[i];
                }
            }
        }
    }

    // Weapon: always passive (requirements checked, SP added to final)
    const wep_req = weapon.get('reqs');
    for (let i = 0; i < 5; i++) {
        if (wep_req[i] > max_passive_req[i]) max_passive_req[i] = wep_req[i];
    }
    const wep_skp = weapon.get('skillpoints');
    for (let i = 0; i < 5; i++) total_item_skillpoints[i] += wep_skp[i];

    // Set bonuses: treated as free (always available)
    for (const [set_name, count] of set_counts) {
        const bonus = sets.get(set_name).bonuses[count - 1];
        for (const i in skp_order) {
            const delta = (bonus[skp_order[i]] || 0);
            free_bonus[i] += delta;
            total_item_skillpoints[i] += delta;
        }
    }

    // ── Phase 2: Trivial fast path (k == 0) ─────────────────────────────────

    if (k === 0) {
        let total_assigned = 0;
        for (let i = 0; i < 5; i++) {
            if (max_passive_req[i] === 0) continue;
            const need = max_passive_req[i] - free_bonus[i];
            if (need > 0) {
                if (need > SP_PER_ATTR_CAP) return null;
                assign[i] = need;
                total_assigned += need;
                if (total_assigned > sp_budget) return null;
            }
        }

        if (scratch_sp) {
            for (let i = 0; i < 5; i++) final_skillpoints[i] = assign[i] + free_bonus[i];
        } else {
            final_skillpoints = [
                assign[0] + free_bonus[0], assign[1] + free_bonus[1],
                assign[2] + free_bonus[2], assign[3] + free_bonus[3],
                assign[4] + free_bonus[4],
            ];
        }
        // Add weapon + crafted SP to final
        const nb_len = scratch_sp ? scratch_sp._no_bonus_len : no_bonus_items.length;
        for (let n = 0; n < nb_len; n++) {
            const skp = no_bonus_items[n].get('skillpoints');
            for (let i = 0; i < 5; i++) final_skillpoints[i] += skp[i];
        }

        return [assign, final_skillpoints, total_assigned, set_counts, total_item_skillpoints];
    }

    // ── Phase 3: Precompute post_floor + backtracking search ──────────────

    // Total ordering bonus across all ordering items
    const total_ord_bonus = [0, 0, 0, 0, 0];
    for (let n = 0; n < k; n++) {
        for (let i = 0; i < 5; i++) total_ord_bonus[i] += ord_skp[n][i];
    }

    // post_floor[j] = minimum final assign[j] after all items activated.
    // Enforces both passive requirements and the bootstrap constraint:
    //   For each ordering item n: assign[j] + free_bonus[j] + total_ord_bonus[j] - skp_n[j] >= req_n[j]
    //   => assign[j] >= req_n[j] + skp_n[j] - free_bonus[j] - total_ord_bonus[j]
    for (let j = 0; j < 5; j++) {
        let floor_j = 0;
        // Passive requirement floor
        if (max_passive_req[j] > 0) {
            floor_j = max_passive_req[j] - free_bonus[j] - total_ord_bonus[j];
        }
        // Bootstrap (self-exclusion) constraint per ordering item
        for (let n = 0; n < k; n++) {
            if (ord_reqs[n][j] > 0) {
                const bs = ord_reqs[n][j] + ord_skp[n][j] - free_bonus[j] - total_ord_bonus[j];
                if (bs > floor_j) floor_j = bs;
            }
        }
        if (floor_j < 0) floor_j = 0;
        post_floor[j] = floor_j;
    }

    // Early reject if post_floor alone exceeds caps/budget
    let lb_total = 0;
    for (let j = 0; j < 5; j++) {
        if (post_floor[j] > SP_PER_ATTR_CAP) return null;
        lb_total += post_floor[j];
    }
    if (lb_total > sp_budget) return null;

    // Backtracking search over activation orderings
    for (let i = 0; i < 5; i++) { running_bonus[i] = 0; assign[i] = 0; }
    let best_total = Infinity;

    // ── Lodestone-style closure fast path ───────────────────────────────────
    //
    // Greedily activate ordering items at assign = post_floor, repeating until
    // no further item becomes activatable. If every ordering item activates,
    // the activation sequence is a path _bt itself could walk on which assign
    // never has to rise above post_floor: each activation demand is
    // req_n - free - running_bonus <= post_floor by the activation test, and
    // each sustain demand is req_m + skp_m - free - running_bonus <= post_floor
    // by the sustain test below. Its leaf value is therefore exactly
    // lb_total = sum(post_floor), which is a lower bound on every leaf, so the
    // ordering search can be skipped. best_assign is unambiguous at that
    // value: any leaf scoring lb_total has best_assign >= post_floor
    // componentwise and summing to sum(post_floor), i.e. post_floor exactly —
    // so this cannot pick a different one of several tied optima than the
    // search would have. (Adapted from the Lodestone algorithm's worst-case
    // greedy fixpoint, SP-Algorithm-Bounty.)
    //
    // The sustain re-check is what lets this run with negative SP lanes
    // present. This path used to be skipped outright whenever any ordering item
    // had one, which was expensive in practice: negative lanes are common on
    // endgame items, and a wide search can hit builds that have one on
    // essentially every leaf, sending all of them into the full k! search.
    //
    // The re-check is only needed when the item being activated is itself the
    // one with a negative lane: activating an item whose lanes are all >= 0 can
    // only raise running_bonus, so anything already sustained stays sustained.
    // neg_mask therefore gates it per item rather than per build — a build with
    // one negative-lane accessory pays the re-check on that one activation, not
    // on all k of them, and an all-non-negative build runs exactly the code it
    // ran before.
    let closure_solved = false;
    {
        let neg_mask = 0;
        for (let n = 0; n < k; n++) {
            const skp_n = ord_skp[n];
            for (let j = 0; j < 5; j++) {
                if (skp_n[j] < 0) { neg_mask |= 1 << n; break; }
            }
        }
        let activated_count = 0;
        let activated_mask = 0;
        let progress = true;
        while (progress && activated_count < k) {
            progress = false;
            for (let n = 0; n < k; n++) {
                if (activated_mask & (1 << n)) continue;
                const req_n = ord_reqs[n];
                let ok = true;
                for (let j = 0; j < 5; j++) {
                    if (req_n[j] > 0
                        && req_n[j] > post_floor[j] + free_bonus[j] + running_bonus[j]) {
                        ok = false;
                        break;
                    }
                }
                if (!ok) continue;

                const skp_n = ord_skp[n];
                for (let j = 0; j < 5; j++) running_bonus[j] += skp_n[j];

                // Items already activated must stay self-sustaining once n's
                // bonus (which may be negative in some lane) is in play —
                // mirrors _bt's intermediate sustainability check, evaluated
                // against the pre-update mask.
                const needs_sustain_check = (neg_mask & (1 << n)) !== 0;
                let sustain_ok = true;
                for (let m = 0; needs_sustain_check && m < k && sustain_ok; m++) {
                    if (!(activated_mask & (1 << m))) continue;
                    const req_m = ord_reqs[m];
                    const skp_m = ord_skp[m];
                    for (let j = 0; j < 5; j++) {
                        if (req_m[j] > 0
                            && req_m[j] + skp_m[j] - free_bonus[j] - running_bonus[j] > post_floor[j]) {
                            sustain_ok = false;
                            break;
                        }
                    }
                }
                if (!sustain_ok) {
                    // Roll back and leave n for a later pass — activating it
                    // now would break an item that is already up.
                    for (let j = 0; j < 5; j++) running_bonus[j] -= skp_n[j];
                    continue;
                }

                activated_mask |= 1 << n;
                activated_count++;
                progress = true;
            }
        }
        for (let j = 0; j < 5; j++) running_bonus[j] = 0;
        if (activated_count === k) {
            closure_solved = true;
            best_total = lb_total;
            for (let j = 0; j < 5; j++) best_assign[j] = post_floor[j];
        }
    }

    function _bt(depth, used, running_total) {
        if (depth === k) {
            // Apply post_floor constraints at leaf
            let ft = running_total;
            for (let j = 0; j < 5; j++) {
                if (post_floor[j] > assign[j]) ft += post_floor[j] - assign[j];
            }
            if (ft < best_total) {
                best_total = ft;
                for (let j = 0; j < 5; j++) {
                    best_assign[j] = post_floor[j] > assign[j] ? post_floor[j] : assign[j];
                }
            }
            return;
        }

        for (let n = 0; n < k; n++) {
            if (used & (1 << n)) continue;

            const req_n = ord_reqs[n];
            const skp_n = ord_skp[n];
            const save_off = depth * 5;

            // Save assign
            for (let j = 0; j < 5; j++) save_stack[save_off + j] = assign[j];

            // Bump assign to meet this item's activation requirements
            let new_total = running_total;
            let cap_ok = true;
            for (let j = 0; j < 5; j++) {
                if (req_n[j] > 0) {
                    const demand = req_n[j] - free_bonus[j] - running_bonus[j];
                    if (demand > assign[j]) {
                        if (demand > SP_PER_ATTR_CAP) { cap_ok = false; break; }
                        new_total += demand - assign[j];
                        assign[j] = demand;
                    }
                }
            }

            if (cap_ok) {
                for (let j = 0; j < 5; j++) running_bonus[j] += skp_n[j];

                // Intermediate sustainability: after applying item n's bonus,
                // all previously activated items must remain self-sustaining
                // (requirements met even without their own bonus contribution).
                let sustain_ok = true;
                for (let m = 0; m < k; m++) {
                    if (!(used & (1 << m))) continue;
                    for (let j = 0; j < 5; j++) {
                        if (ord_reqs[m][j] > 0) {
                            const demand = ord_reqs[m][j] + ord_skp[m][j] - free_bonus[j] - running_bonus[j];
                            if (demand > assign[j]) {
                                if (demand > SP_PER_ATTR_CAP) { sustain_ok = false; break; }
                                new_total += demand - assign[j];
                                assign[j] = demand;
                            }
                        }
                    }
                    if (!sustain_ok) break;
                }

                if (sustain_ok) {
                    // Lower-bound pruning: current total + remaining post_floor gaps
                    let lb = new_total;
                    for (let j = 0; j < 5; j++) {
                        if (post_floor[j] > assign[j]) lb += post_floor[j] - assign[j];
                    }
                    if (lb < best_total) {
                        _bt(depth + 1, used | (1 << n), new_total);
                    }
                }

                for (let j = 0; j < 5; j++) running_bonus[j] -= skp_n[j];
            }

            // Restore assign
            for (let j = 0; j < 5; j++) assign[j] = save_stack[save_off + j];
        }
    }

    if (!closure_solved) _bt(0, 0, 0);

    if (best_total === Infinity) return null;

    // ── Phase 4: Finalization ───────────────────────────────────────────────

    for (let i = 0; i < 5; i++) assign[i] = best_assign[i];

    // Cap + budget check
    let total_assigned = 0;
    for (let i = 0; i < 5; i++) {
        if (assign[i] > SP_PER_ATTR_CAP) return null;
        total_assigned += assign[i];
        if (total_assigned > sp_budget) return null;
    }

    // Final SP = assign + free_bonus + total_ordering_bonus
    if (scratch_sp) {
        for (let i = 0; i < 5; i++) {
            final_skillpoints[i] = assign[i] + free_bonus[i] + total_ord_bonus[i];
        }
    } else {
        final_skillpoints = [0, 0, 0, 0, 0];
        for (let i = 0; i < 5; i++) {
            final_skillpoints[i] = assign[i] + free_bonus[i] + total_ord_bonus[i];
        }
    }

    // Add weapon + crafted SP to final
    const nb_len = scratch_sp ? scratch_sp._no_bonus_len : no_bonus_items.length;
    for (let n = 0; n < nb_len; n++) {
        const skp = no_bonus_items[n].get('skillpoints');
        for (let i = 0; i < 5; i++) final_skillpoints[i] += skp[i];
    }

    return [assign, final_skillpoints, total_assigned, set_counts, total_item_skillpoints];
}
