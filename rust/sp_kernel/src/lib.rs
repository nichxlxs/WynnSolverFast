//! SP kernel — Rust port of `js/game/skillpoints.js::calculate_skillpoints` (P2.2).
//!
//! Reads fixtures exported by `js/solver/tests/gen_rust_sp_fixtures.js`,
//! verifies exact parity with the JS results on every case, then reports
//! throughput for the kernel alone.
//!
//! Usage: sp_kernel <fixtures.txt> [bench_iters]

pub const SP_PER_ATTR_CAP: i32 = 100;

#[derive(Clone, Copy, Default)]
pub struct Unit {
    pub crafted: bool,
    pub reqs: [i32; 5],
    pub skp: [i32; 5],
}

#[derive(Clone, Copy)]
pub struct Case {
    pub budget: i32,
    pub equipment: [Unit; 8],
    pub weapon: Unit,
    pub set_free: [i32; 5],
    pub expected: Option<([i32; 5], [i32; 5], i32)>,
}

pub struct Kernel {
    ord_reqs: [[i32; 5]; 9],
    ord_skp: [[i32; 5]; 9],
    best_assign: [i32; 5],
    save_stack: [i32; 45],
}

impl Kernel {
    pub fn new() -> Self {
        Kernel {
            ord_reqs: [[0; 5]; 9],
            ord_skp: [[0; 5]; 9],
            best_assign: [0; 5],
            save_stack: [0; 45],
        }
    }

    /// Returns Some((assign, final_sp, total_assigned)) or None (infeasible).
    pub fn calculate(&mut self, case: &Case) -> Option<([i32; 5], [i32; 5], i32)> {
        self.calculate_with_extra(case, None)
    }

    /// Same as calculate, with an optional 9th equipment unit (the guild
    /// tome slot in the solver's calculate_skillpoints input).
    pub fn calculate_with_extra(&mut self, case: &Case, extra: Option<&Unit>) -> Option<([i32; 5], [i32; 5], i32)> {
        let mut free_bonus = [0i32; 5];
        let mut max_passive_req = [0i32; 5];
        let mut no_bonus_skp_sum = [0i32; 5]; // weapon + crafted, added to final
        let mut k = 0usize;

        for j in 0..5 {
            no_bonus_skp_sum[j] += case.weapon.skp[j];
        }

        for item in case.equipment.iter().chain(extra) {
            if item.crafted {
                for j in 0..5 {
                    no_bonus_skp_sum[j] += item.skp[j];
                    if item.reqs[j] > max_passive_req[j] {
                        max_passive_req[j] = item.reqs[j];
                    }
                }
            } else {
                let has_req = item.reqs.iter().any(|&r| r > 0);
                let has_skp = item.skp.iter().any(|&s| s != 0);
                if has_req && has_skp {
                    self.ord_reqs[k] = item.reqs;
                    self.ord_skp[k] = item.skp;
                    k += 1;
                } else if !has_req {
                    for j in 0..5 {
                        free_bonus[j] += item.skp[j];
                    }
                } else {
                    for j in 0..5 {
                        if item.reqs[j] > max_passive_req[j] {
                            max_passive_req[j] = item.reqs[j];
                        }
                    }
                }
            }
        }

        // Weapon: passive requirements.
        for j in 0..5 {
            if case.weapon.reqs[j] > max_passive_req[j] {
                max_passive_req[j] = case.weapon.reqs[j];
            }
        }
        // Set bonuses: free pool (precomputed by the exporter).
        for j in 0..5 {
            free_bonus[j] += case.set_free[j];
        }

        // Trivial fast path (k == 0).
        if k == 0 {
            let mut assign = [0i32; 5];
            let mut total = 0i32;
            for j in 0..5 {
                if max_passive_req[j] == 0 {
                    continue;
                }
                let need = max_passive_req[j] - free_bonus[j];
                if need > 0 {
                    if need > SP_PER_ATTR_CAP {
                        return None;
                    }
                    assign[j] = need;
                    total += need;
                    if total > case.budget {
                        return None;
                    }
                }
            }
            let mut final_sp = [0i32; 5];
            for j in 0..5 {
                final_sp[j] = assign[j] + free_bonus[j] + no_bonus_skp_sum[j];
            }
            return Some((assign, final_sp, total));
        }

        // post_floor: end-state lower bound incl. bootstrap self-exclusion.
        let mut total_ord_bonus = [0i32; 5];
        for n in 0..k {
            for j in 0..5 {
                total_ord_bonus[j] += self.ord_skp[n][j];
            }
        }
        let mut post_floor = [0i32; 5];
        for j in 0..5 {
            let mut floor_j = 0i32;
            if max_passive_req[j] > 0 {
                floor_j = max_passive_req[j] - free_bonus[j] - total_ord_bonus[j];
            }
            for n in 0..k {
                if self.ord_reqs[n][j] > 0 {
                    let bs = self.ord_reqs[n][j] + self.ord_skp[n][j]
                        - free_bonus[j]
                        - total_ord_bonus[j];
                    if bs > floor_j {
                        floor_j = bs;
                    }
                }
            }
            post_floor[j] = floor_j.max(0);
        }

        let mut lb_total = 0i32;
        for j in 0..5 {
            if post_floor[j] > SP_PER_ATTR_CAP {
                return None;
            }
            lb_total += post_floor[j];
        }
        if lb_total > case.budget {
            return None;
        }

        // Lodestone-style closure fast path (see skillpoints.js).
        let mut best_total = i32::MAX;
        let mut closure_solved = false;
        let has_neg_ord = (0..k).any(|n| self.ord_skp[n].iter().any(|&s| s < 0));
        if !has_neg_ord {
            let mut running = [0i32; 5];
            let mut mask = 0u32;
            let mut count = 0usize;
            let mut progress = true;
            while progress && count < k {
                progress = false;
                for n in 0..k {
                    if mask & (1 << n) != 0 {
                        continue;
                    }
                    let ok = (0..5).all(|j| {
                        self.ord_reqs[n][j] <= 0
                            || self.ord_reqs[n][j] <= post_floor[j] + free_bonus[j] + running[j]
                    });
                    if ok {
                        for j in 0..5 {
                            running[j] += self.ord_skp[n][j];
                        }
                        mask |= 1 << n;
                        count += 1;
                        progress = true;
                    }
                }
            }
            if count == k {
                closure_solved = true;
                best_total = lb_total;
                self.best_assign = post_floor;
            }
        }

        if !closure_solved {
            let mut assign = [0i32; 5];
            let mut running_bonus = [0i32; 5];
            self.bt(
                0, 0, 0, k, &free_bonus, &post_floor, &mut assign, &mut running_bonus,
                &mut best_total,
            );
        }

        if best_total == i32::MAX {
            return None;
        }

        let assign = self.best_assign;
        let mut total_assigned = 0i32;
        for j in 0..5 {
            if assign[j] > SP_PER_ATTR_CAP {
                return None;
            }
            total_assigned += assign[j];
            if total_assigned > case.budget {
                return None;
            }
        }

        let mut final_sp = [0i32; 5];
        for j in 0..5 {
            final_sp[j] =
                assign[j] + free_bonus[j] + total_ord_bonus[j] + no_bonus_skp_sum[j];
        }
        Some((assign, final_sp, total_assigned))
    }

    #[allow(clippy::too_many_arguments)]
    fn bt(
        &mut self,
        depth: usize,
        used: u32,
        running_total: i32,
        k: usize,
        free_bonus: &[i32; 5],
        post_floor: &[i32; 5],
        assign: &mut [i32; 5],
        running_bonus: &mut [i32; 5],
        best_total: &mut i32,
    ) {
        if depth == k {
            let mut ft = running_total;
            for j in 0..5 {
                if post_floor[j] > assign[j] {
                    ft += post_floor[j] - assign[j];
                }
            }
            if ft < *best_total {
                *best_total = ft;
                for j in 0..5 {
                    self.best_assign[j] = post_floor[j].max(assign[j]);
                }
            }
            return;
        }

        for n in 0..k {
            if used & (1 << n) != 0 {
                continue;
            }
            let req_n = self.ord_reqs[n];
            let skp_n = self.ord_skp[n];
            let save_off = depth * 5;
            for j in 0..5 {
                self.save_stack[save_off + j] = assign[j];
            }

            let mut new_total = running_total;
            let mut cap_ok = true;
            for j in 0..5 {
                if req_n[j] > 0 {
                    let demand = req_n[j] - free_bonus[j] - running_bonus[j];
                    if demand > assign[j] {
                        if demand > SP_PER_ATTR_CAP {
                            cap_ok = false;
                            break;
                        }
                        new_total += demand - assign[j];
                        assign[j] = demand;
                    }
                }
            }

            if cap_ok {
                for j in 0..5 {
                    running_bonus[j] += skp_n[j];
                }
                let mut sustain_ok = true;
                for m in 0..k {
                    if used & (1 << m) == 0 {
                        continue;
                    }
                    for j in 0..5 {
                        if self.ord_reqs[m][j] > 0 {
                            let demand = self.ord_reqs[m][j] + self.ord_skp[m][j]
                                - free_bonus[j]
                                - running_bonus[j];
                            if demand > assign[j] {
                                if demand > SP_PER_ATTR_CAP {
                                    sustain_ok = false;
                                    break;
                                }
                                new_total += demand - assign[j];
                                assign[j] = demand;
                            }
                        }
                    }
                    if !sustain_ok {
                        break;
                    }
                }

                if sustain_ok {
                    let mut lb = new_total;
                    for j in 0..5 {
                        if post_floor[j] > assign[j] {
                            lb += post_floor[j] - assign[j];
                        }
                    }
                    if lb < *best_total {
                        self.bt(
                            depth + 1, used | (1 << n), new_total, k, free_bonus,
                            post_floor, assign, running_bonus, best_total,
                        );
                    }
                }

                for j in 0..5 {
                    running_bonus[j] -= skp_n[j];
                }
            }

            for j in 0..5 {
                assign[j] = self.save_stack[save_off + j];
            }
        }
    }
}

extern crate self as sp_kernel;

pub mod engine;
pub mod scoring;
pub mod search_core;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn solve_json(input: &str) -> String {
    engine::solve_json(input)
}
