/* @ts-self-types="./sp_kernel.d.ts" */

/**
 * R24: one worker's engine for the work-queue solve.
 *
 * Browser workers cannot share a cutoff without `SharedArrayBuffer`, and a
 * running solve cannot receive messages, so `solve_partition` workers each
 * rediscover their own cutoff. An `Engine` instead parses the fixtures,
 * builds the bound tables and runs the warm start once, then solves units
 * (contiguous first-slot offset ranges) one call at a time. Between calls
 * the host hands the worker its next unit together with the merged cutoff
 * of everything every worker has finished, so later units prune against
 * the best builds found anywhere. Results accumulate across a worker's
 * units: each call returns the solve JSON of all of them so far, in the
 * same shape `solve_partition` returns, and progress is cumulative too.
 */
export class Engine {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        EngineFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_engine_free(ptr, 0);
    }
    /**
     * @param {string} enum_fixture
     * @param {string} score_fixture
     */
    constructor(enum_fixture, score_fixture) {
        const ptr0 = passStringToWasm0(enum_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(score_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.engine_new(ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        EngineFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * How many entries the merged cutoff counts down to: the result count,
     * or the archive cap under an R20 window.
     * @returns {number}
     */
    result_count() {
        const ret = wasm.engine_result_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Solves unit `index` of `count`. `seed_cutoff` is the floor of the
     * `result_count()`-th best distinct score across every worker's latest
     * report (0 for none) and `seed_best` the best; both are admissible
     * because they are scores of real builds.
     * @param {number} index
     * @param {number} count
     * @param {number} seed_cutoff
     * @param {number} seed_best
     * @param {Function} on_progress
     * @returns {string}
     */
    solve_unit(index, count, seed_cutoff, seed_best, on_progress) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.engine_solve_unit(this.__wbg_ptr, index, count, seed_cutoff, seed_best, on_progress);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Engine.prototype[Symbol.dispose] = Engine.prototype.free;

/**
 * Total canonical search size for a fixture, so the UI can show progress
 * without starting a solve.
 * @param {string} enum_fixture
 * @returns {number}
 */
export function search_space(enum_fixture) {
    const ptr0 = passStringToWasm0(enum_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.search_space(ptr0, len0);
    return ret;
}

/**
 * Solve a scenario and return results as a JSON string. Thin bindgen shim
 * over `enumerate::solve_json`, which is also callable natively so the
 * browser path can be tested without a browser.
 *
 * `max_leaves <= 0` means "run to completion".
 * @param {string} enum_fixture
 * @param {string} score_fixture
 * @param {number} max_leaves
 * @returns {string}
 */
export function solve(enum_fixture, score_fixture, max_leaves) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(enum_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(score_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.solve(ptr0, len0, ptr1, len1, max_leaves);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Search overlapping neighborhoods for strong builds within a time budget.
 * This is a heuristic: every progress/final payload sets `complete:false`.
 * Options and witness shapes are shared with the native testable wrapper.
 * @param {string} enum_fixture
 * @param {string} score_fixture
 * @param {string} options_json
 * @param {Function} on_progress
 * @returns {string}
 */
export function solve_anytime_with_progress(enum_fixture, score_fixture, options_json, on_progress) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passStringToWasm0(enum_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(score_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(options_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.solve_anytime_with_progress(ptr0, len0, ptr1, len1, ptr2, len2, on_progress);
        deferred4_0 = ret[0];
        deferred4_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * `solve_with_progress` restricted to one partition of the search space.
 *
 * wasm threads need `SharedArrayBuffer` and COOP/COEP cross-origin
 * isolation, which the app cannot assume. Partitioning needs neither: the
 * host spawns one ordinary worker per core, each running this with its own
 * `part_index`, and merges the results. The split is by first-slot offset —
 * the same one the native threaded path work-steals over — so the
 * partitions are disjoint, `checked` sums to the whole space, and the
 * merged top-N is identical to a single-partition run (`partition_check`).
 *
 * Each partition still reports the FULL space as `total`, so a host summing
 * `checked` across workers gets a coherent percentage.
 *
 * The one thing lost versus native threads is the shared score cutoff: each
 * partition discovers its own, so the gate prunes a little less. That costs
 * work, never results.
 * @param {string} enum_fixture
 * @param {string} score_fixture
 * @param {number} max_leaves
 * @param {number} part_index
 * @param {number} part_count
 * @param {Function} on_progress
 * @returns {string}
 */
export function solve_partition(enum_fixture, score_fixture, max_leaves, part_index, part_count, on_progress) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(enum_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(score_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.solve_partition(ptr0, len0, ptr1, len1, max_leaves, part_index, part_count, on_progress);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * `solve` with a live-progress callback.
 *
 * `on_progress` is invoked with a JSON string (checked/total, the funnel
 * counters, and the interim top-N) roughly every 2M credited leaves and
 * once more when the search ends. This is what lets a long solve show
 * movement in the UI instead of appearing hung — the reason to run this in
 * a dedicated worker rather than chunking on the main thread.
 *
 * Exact-mode emission is keyed on credited leaves, preserving its existing
 * progress behavior and deterministic emission points.
 * @param {string} enum_fixture
 * @param {string} score_fixture
 * @param {number} max_leaves
 * @param {Function} on_progress
 * @returns {string}
 */
export function solve_with_progress(enum_fixture, score_fixture, max_leaves, on_progress) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(enum_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(score_fixture, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.solve_with_progress(ptr0, len0, ptr1, len1, max_leaves, on_progress);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_number_get_136b9679cab35cfb: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_throw_bb96b2010945f0bc: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_call_35dba3c747ad7521: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_now_4b23a1420c8a31f6: function() {
            const ret = performance.now();
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./sp_kernel_bg.js": import0,
    };
}

const EngineFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_engine_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('sp_kernel_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
