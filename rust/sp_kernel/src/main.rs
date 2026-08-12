//! Rust port of `js/game/skillpoints.js::calculate_skillpoints` (P2.2).
//!
//! Reads fixtures exported by `js/solver/tests/gen_rust_sp_fixtures.js`,
//! verifies exact parity with the JS results on every case, then reports
//! throughput for the kernel alone.
//!
//! Usage: sp_kernel <fixtures.txt> [bench_iters]

use std::env;
use std::fs;
use std::time::Instant;

const SP_PER_ATTR_CAP: i32 = 100;

#[derive(Clone, Copy, Default)]
struct Unit {
    crafted: bool,
    reqs: [i32; 5],
    skp: [i32; 5],
}

#[derive(Clone, Copy)]
struct Case {
    budget: i32,
    equipment: [Unit; 8],
    weapon: Unit,
    set_free: [i32; 5],
    expected: Option<([i32; 5], [i32; 5], i32)>,
}

struct Kernel {
    ord_reqs: [[i32; 5]; 9],
    ord_skp: [[i32; 5]; 9],
    best_assign: [i32; 5],
    save_stack: [i32; 45],
}

impl Kernel {
    fn new() -> Self {
        Kernel {
            ord_reqs: [[0; 5]; 9],
            ord_skp: [[0; 5]; 9],
            best_assign: [0; 5],
            save_stack: [0; 45],
        }
    }

    /// Returns Some((assign, final_sp, total_assigned)) or None (infeasible).
    fn calculate(&mut self, case: &Case) -> Option<([i32; 5], [i32; 5], i32)> {
        let mut free_bonus = [0i32; 5];
        let mut max_passive_req = [0i32; 5];
        let mut no_bonus_skp_sum = [0i32; 5]; // weapon + crafted, added to final
        let mut k = 0usize;

        for j in 0..5 {
            no_bonus_skp_sum[j] += case.weapon.skp[j];
        }

        for item in &case.equipment {
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

fn parse_cases(text: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    for line in text.lines() {
        let nums: Vec<i32> = line
            .split_ascii_whitespace()
            .map(|t| t.parse().expect("bad int"))
            .collect();
        if nums.is_empty() {
            continue;
        }
        let mut i = 0usize;
        let mut next = || {
            let v = nums[i];
            i += 1;
            v
        };
        let budget = next();
        let mut read_unit = || {
            let crafted = next() != 0;
            let mut reqs = [0i32; 5];
            let mut skp = [0i32; 5];
            for r in reqs.iter_mut() {
                *r = next();
            }
            for s in skp.iter_mut() {
                *s = next();
            }
            Unit { crafted, reqs, skp }
        };
        let mut equipment = [Unit::default(); 8];
        for e in equipment.iter_mut() {
            *e = read_unit();
        }
        let weapon = read_unit();
        let mut set_free = [0i32; 5];
        for b in set_free.iter_mut() {
            *b = next();
        }
        let feas = next() != 0;
        let mut assign = [0i32; 5];
        let mut final_sp = [0i32; 5];
        for a in assign.iter_mut() {
            *a = next();
        }
        for f in final_sp.iter_mut() {
            *f = next();
        }
        let total = next();
        let expected = if feas {
            Some((assign, final_sp, total))
        } else {
            None
        };
        cases.push(Case {
            budget,
            equipment,
            weapon,
            set_free,
            expected,
        });
    }
    cases
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("fixtures/sp_cases.txt");
    let bench_iters: usize = args
        .get(2)
        .map(|s| s.parse().expect("bad iters"))
        .unwrap_or(2000);

    let text = fs::read_to_string(fixture_path).expect("cannot read fixtures");
    let cases = parse_cases(&text);
    let mut kernel = Kernel::new();

    // Parity check.
    let mut mismatches = 0usize;
    let mut feasible = 0usize;
    for (idx, case) in cases.iter().enumerate() {
        let got = kernel.calculate(case);
        if got != case.expected {
            mismatches += 1;
            if mismatches <= 5 {
                eprintln!(
                    "MISMATCH case {}: got {:?}, expected {:?}",
                    idx, got, case.expected
                );
            }
        }
        if got.is_some() {
            feasible += 1;
        }
    }
    println!(
        "parity: {} cases, {} feasible, {} mismatches",
        cases.len(),
        feasible,
        mismatches
    );
    if mismatches > 0 {
        std::process::exit(1);
    }

    // Bench: kernel-only throughput.
    let start = Instant::now();
    let mut sink = 0i64;
    for _ in 0..bench_iters {
        for case in &cases {
            if let Some((_, _, total)) = kernel.calculate(case) {
                sink += total as i64;
            }
        }
    }
    let elapsed = start.elapsed();
    let calls = bench_iters * cases.len();
    println!(
        "bench: {} calls in {:.1} ms => {:.0} calls/s ({:.3} us/call, checksum {})",
        calls,
        elapsed.as_secs_f64() * 1e3,
        calls as f64 / elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1e6 / calls as f64,
        sink
    );
}
