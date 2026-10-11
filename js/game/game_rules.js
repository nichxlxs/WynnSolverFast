/**
 * Centralized game rule constants.
 * Values derived from Wynncraft game mechanics.
 */

// ── Skill Points ──────────────────────────────────────────────────────────────
const SP_TOTAL_CAP = 200;           // Max assignable skill points
const SP_PER_ATTR_CAP = 100;        // Max skill points per attribute
// Guild tomes do NOT raise the assignable budget. They grant fixed
// per-attribute skill points and are modelled as item statMaps (see
// GUILD_TOMES in js/solver/constants.js). These two constants encoded the old
// "inflate the budget" model, which allowed the bonus to be split across
// attributes in a way no real tome can produce; both are unused.
const SP_GUILD_TOME_STD = 204;      // DEPRECATED — do not use for new code
const SP_GUILD_TOME_RARE = 205;     // DEPRECATED — never used
const SP_PERCENTAGE_RATE = 0.9908;  // Geometric series rate for SP→% conversion
const SP_PERCENTAGE_INPUT_CAP = 150;// SP input cap for percentage conversion

// ── Player Stats ──────────────────────────────────────────────────────────────
const MAX_PLAYER_LEVEL = 121;

// ── Mana ─────────────────────────────────────────────────────────────────────
const BASE_MANA_REGEN = 25;        // Base mana regen per 5 seconds (all classes)
const MANA_TICK_SECONDS = 5;       // Mana regen tick period (seconds)
// Max mana pool, base 100 and the Intelligence bonus included. The official
// wiki's Identifying page: "Max Mana caps at 400, including the bonuses
// provided by Intelligence and Base Mana (100)." Shipped to the Rust engine
// in its tables (max_mana_cap); Infinity turns the cap off in both engines.
const MAX_MANA_CAP = 400;

/** Max (and starting) mana: 100 + Max Mana IDs + Int bonus, capped. */
function total_mana_pool(item_mana, int_mana) {
    return Math.min(MAX_MANA_CAP, 100 + item_mana + int_mana);
}

// ── Health ───────────────────────────────────────────────────────────────────
const HIDDEN_BASE_HPR = 3;         // Hidden base HP regen per tick (not affected by hprPct)
const HPR_TICK_SECONDS = 4;        // HP regen tick period (seconds)

// ── Spell Timing ─────────────────────────────────────────────────────────────
const SPELL_CAST_TIME = 0.3;       // Spell cast time (seconds)
const SPELL_CAST_DELAY = 0.1;      // Delay between spell casts (seconds)
