//! Direct FFI tests proving Eris works end-to-end before we layer
//! anything on top. Each test opens a Lua state via raw FFI, executes
//! a script, persists, restores into a fresh state, and validates the
//! restored state matches the original.
//!
//! No `mlua` involvement — these tests prove the bundled C library is
//! correct in isolation.

use lua_eris_sys::*;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

/// Convert a Rust `&str` to a CString and pass its pointer to a closure.
/// Saves a lot of `CString::new(...).unwrap()` boilerplate.
fn with_cstr<R>(s: &str, f: impl FnOnce(*const c_char) -> R) -> R {
    let c = CString::new(s).expect("string contained null byte");
    f(c.as_ptr())
}

/// Open a Lua state with all standard libraries loaded.
unsafe fn new_lua_state() -> *mut lua_State {
    let L = luaL_newstate();
    assert!(!L.is_null(), "failed to allocate Lua state");
    luaL_openlibs(L);
    L
}

/// Run a Lua script string in the given state. Returns the int result
/// of `lua_pcall` (0 == OK). On error, panics with the Lua error message.
unsafe fn run_script(L: *mut lua_State, script: &str) {
    let load_rc = with_cstr(script, |p| luaL_loadstring(L, p));
    if load_rc != LUA_OK {
        let msg = lua_tostring(L, -1);
        panic!(
            "luaL_loadstring failed (rc={}): {}",
            load_rc,
            CStr::from_ptr(msg).to_string_lossy()
        );
    }
    let call_rc = lua_pcall(L, 0, LUA_MULTRET, 0);
    if call_rc != LUA_OK {
        let msg = lua_tostring(L, -1);
        panic!(
            "lua_pcall failed (rc={}): {}",
            call_rc,
            CStr::from_ptr(msg).to_string_lossy()
        );
    }
}

/// Push an empty perms table onto the stack and return its index.
/// Eris needs a perms table even when no values need to be excluded —
/// it's the table that maps "this Lua value cannot be persisted, use this
/// stable string key instead." For pure-data tests, an empty table works.
unsafe fn push_empty_perms(L: *mut lua_State) -> c_int {
    lua_createtable(L, 0, 0);
    lua_gettop(L)
}

/// Helper: read a Lua string value at the given stack index into a Rust Vec<u8>.
unsafe fn read_lua_string_bytes(L: *mut lua_State, idx: c_int) -> Vec<u8> {
    let mut len: usize = 0;
    let ptr = lua_tolstring(L, idx, &mut len as *mut usize);
    assert!(!ptr.is_null(), "expected string at stack index {}", idx);
    std::slice::from_raw_parts(ptr as *const u8, len).to_vec()
}

/// Push a Vec<u8> onto the stack as a Lua string.
unsafe fn push_bytes(L: *mut lua_State, bytes: &[u8]) {
    // Use lua_pushlstring via lua_tolstring's symmetric — actually
    // there's no lua_pushlstring in our FFI yet. Push via a CString
    // (no embedded nulls in eris output? actually there ARE nulls in
    // binary blobs). We need lua_pushlstring.
    extern "C" {
        fn lua_pushlstring(L: *mut lua_State, s: *const c_char, len: usize) -> *const c_char;
    }
    lua_pushlstring(L, bytes.as_ptr() as *const c_char, bytes.len());
}

// ============================================================
// Test 1: Persist/unpersist a primitive value (sanity check)
// ============================================================
#[test]
fn persist_a_simple_integer() {
    unsafe {
        let L = new_lua_state();

        // Push the integer 42 onto the stack at index 1.
        lua_pushinteger(L, 42);
        let value_idx = lua_gettop(L);
        let perms_idx = push_empty_perms(L);

        // Persist: push a serialized string onto the stack.
        let pre = lua_gettop(L);
        eris_persist(L, perms_idx, value_idx);
        let post = lua_gettop(L);
        assert_eq!(
            post,
            pre + 1,
            "eris_persist must push exactly one string onto the stack"
        );

        // Pull the bytes out.
        let blob = read_lua_string_bytes(L, -1);
        assert!(
            !blob.is_empty(),
            "serialized blob should be non-empty for the integer 42"
        );
        eprintln!("integer 42 serialized to {} bytes", blob.len());

        lua_close(L);

        // Now create a FRESH state and unpersist into it.
        let L2 = new_lua_state();
        let uperms_idx = push_empty_perms(L2);
        push_bytes(L2, &blob);
        let blob_idx = lua_gettop(L2);

        eris_unpersist(L2, uperms_idx, blob_idx);
        let restored = lua_tointegerx(L2, -1, ptr::null_mut());
        assert_eq!(restored, 42, "round-trip failed for integer 42");

        lua_close(L2);
    }
}

// ============================================================
// Test 2a: Pure-numeric table (no strings, so no string-metatable
// reference). Sanity check that the basic table format works.
// ============================================================
#[test]
fn persist_a_pure_numeric_table() {
    unsafe {
        let L = new_lua_state_with_helpers();
        run_script(L, "t = { 1, 2, 3 }");
        with_cstr("t", |p| lua_getglobal(L, p));
        let blob = persist_top(L);
        eprintln!("numeric table serialized to {} bytes", blob.len());
        lua_close(L);

        let L2 = new_lua_state_with_helpers();
        unpersist_to_top(L2, &blob);
        with_cstr("restored", |p| lua_setglobal(L2, p));

        run_script(
            L2,
            r#"
            assert(restored[1] == 1)
            assert(restored[2] == 2)
            assert(restored[3] == 3)
            "#,
        );
        lua_close(L2);
    }
}

// ============================================================
// Test 2b: Table with string contents (exercises string metatable).
// ============================================================
#[test]
fn persist_a_table_with_string_and_int() {
    unsafe {
        let L = new_lua_state_with_helpers();
        run_script(L, r#"t = { name = "hello", n = 7 }"#);
        with_cstr("t", |p| lua_getglobal(L, p));
        let blob = persist_top(L);
        eprintln!("string-keyed table serialized to {} bytes", blob.len());
        lua_close(L);

        let L2 = new_lua_state_with_helpers();
        unpersist_to_top(L2, &blob);
        with_cstr("restored", |p| lua_setglobal(L2, p));

        run_script(
            L2,
            r#"
            assert(type(restored) == "table",
                "restored value should be a table, got " .. type(restored))
            assert(restored.name == "hello",
                "restored.name should be 'hello', got " .. tostring(restored.name))
            assert(restored.n == 7,
                "restored.n should be 7, got " .. tostring(restored.n))
            "#,
        );

        lua_close(L2);
    }
}

// ============================================================
// Helpers for persist/unpersist via the Lua-level eris API.
// ============================================================
//
// The C-level eris_persist/eris_unpersist raise Lua errors via longjmp.
// Calling them from Rust outside a lua_pcall causes SIGABRT. Calling
// the Lua-level eris.persist/eris.unpersist functions from inside a
// pcall'd Lua script is naturally safe — the pcall catches the error.
//
// Additionally, every C function reachable from the value being
// persisted must be in the perms table. The standard library is full
// of light C functions (coroutine.yield, table.insert, etc.) so we
// pre-build a perms table that maps every stdlib function to a stable
// string key. The same table inverted is used on unpersist.

/// The Lua source for `_build_perms()`, which builds a deterministic
/// perms table mapping every standard library function (and the library
/// tables themselves) to a stable string key.
///
/// **Critical correctness rules:**
/// 1. The perms tables on PERSIST and UNPERSIST must contain the SAME
///    set of (value, key) pairs. If a value appears in perms on one
///    side but not the other, persist will write a perm reference that
///    unpersist can't resolve ("bad permanent value").
/// 2. We must NOT include arbitrary _G contents. Walking _G generically
///    captures user globals (like the value being persisted itself!),
///    causing Eris to serialize it AS a perm reference rather than
///    data — which then fails on unpersist because the user global
///    doesn't exist in the fresh state.
/// 3. Iteration order must be deterministic. `pairs()` doesn't
///    guarantee order, so we sort table-key lists with `table.sort`.
///
/// We register: standard library tables (coroutine, math, etc.) plus
/// a hardcoded list of top-level _G functions (print, type, pcall,
/// pairs, etc.). This list must mirror Lua 5.3's stdlib exactly.
const BUILD_PERMS_LUA: &str = r#"
-- Top-level _G functions (Lua 5.3 base library).
local _BASE_FUNCS = {
    "assert", "collectgarbage", "dofile", "error", "getmetatable",
    "ipairs", "load", "loadfile", "next", "pairs", "pcall", "print",
    "rawequal", "rawget", "rawlen", "rawset", "require", "select",
    "setmetatable", "tonumber", "tostring", "type", "xpcall",
}

-- Standard library tables to register and walk children of.
local _LIBS = {
    "coroutine", "debug", "io", "math",
    "os", "package", "string", "table", "utf8",
    -- "eris" itself is loaded so we register it too — coroutines that
    -- reference eris.persist (unlikely but possible) would need it.
    "eris",
}

function _build_perms()
    local perms = {}
    local function register(value, key)
        -- Only register the first occurrence; same value at multiple
        -- paths keeps its first-seen key.
        if perms[value] == nil and value ~= nil then
            perms[value] = key
        end
    end

    -- Top-level base functions
    for _, name in ipairs(_BASE_FUNCS) do
        register(_G[name], name)
    end

    -- Each stdlib library + its members
    for _, lib_name in ipairs(_LIBS) do
        local lib = _G[lib_name]
        if lib ~= nil then
            register(lib, lib_name)
            if type(lib) == "table" then
                -- Deterministic order over string keys
                local keys = {}
                for k in pairs(lib) do
                    if type(k) == "string" then
                        keys[#keys + 1] = k
                    end
                end
                table.sort(keys)
                for _, k in ipairs(keys) do
                    local v = lib[k]
                    local ty = type(v)
                    if ty == "function" or ty == "table" then
                        register(v, lib_name .. "." .. k)
                    end
                end
            end
        end
    end

    -- _G itself — Eris may need to reference it as a perm.
    register(_G, "_G")

    return perms
end

function _build_uperms()
    local perms = _build_perms()
    local uperms = {}
    for v, k in pairs(perms) do
        uperms[k] = v
    end
    return uperms
end

function _persist(value)
    return eris.persist(_build_perms(), value)
end

function _unpersist(blob)
    return eris.unpersist(_build_uperms(), blob)
end

-- Diagnostic: count perms entries.
function _perms_size()
    local n = 0
    for _ in pairs(_build_perms()) do n = n + 1 end
    return n
end
"#;

/// Initialize a Lua state with the helper functions loaded.
unsafe fn new_lua_state_with_helpers() -> *mut lua_State {
    let L = new_lua_state();
    run_script(L, BUILD_PERMS_LUA);
    L
}

/// Call `_persist(value)` on the value at the top of the stack.
/// Returns the serialized blob as a Vec<u8>. Panics on any Lua error.
unsafe fn persist_top(L: *mut lua_State) -> Vec<u8> {
    // Stack: [..., value]
    // We need to call _persist(value), which pops value and pushes blob.
    // Move the value to a temporary, push _persist, push value, pcall.
    let value_abs = lua_gettop(L);

    with_cstr("_persist", |p| lua_getglobal(L, p));
    // Stack: [..., value, _persist]
    lua_pushvalue(L, value_abs);
    // Stack: [..., value, _persist, value]
    let rc = lua_pcall(L, 1, 1, 0);
    if rc != LUA_OK {
        let msg = lua_tostring(L, -1);
        panic!(
            "eris.persist failed (rc={}): {}",
            rc,
            CStr::from_ptr(msg).to_string_lossy()
        );
    }
    // Stack: [..., value, blob]
    let blob = read_lua_string_bytes(L, -1);
    // Drop blob and original value.
    lua_pop(L, 2);
    blob
}

/// Call `_unpersist(blob)` and leave the restored value on the stack.
unsafe fn unpersist_to_top(L: *mut lua_State, blob: &[u8]) {
    with_cstr("_unpersist", |p| lua_getglobal(L, p));
    push_bytes(L, blob);
    let rc = lua_pcall(L, 1, 1, 0);
    if rc != LUA_OK {
        let msg = lua_tostring(L, -1);
        panic!(
            "eris.unpersist failed (rc={}): {}",
            rc,
            CStr::from_ptr(msg).to_string_lossy()
        );
    }
}

// ============================================================
// Test 3: THE KEY TEST — persist a suspended coroutine and resume it.
// ============================================================
//
// This is the whole reason we're doing this migration. If this test
// fails, the entire premise of switching to Eris is invalid. The test:
//
//   1. Create a coroutine in state A that yields 3 values: 10, 20, 30.
//      Each value is yielded from a deeply-nested call (proves stackful
//      bubble-up works, which is the model Rhai cannot do).
//   2. Resume it twice — get 10, then 20.
//   3. Persist the suspended coroutine.
//   4. Open state B (fresh).
//   5. Unpersist into state B.
//   6. Resume in state B.
//   7. Verify the resumed value is 30 (the third yield), not 10 (would
//      indicate the coroutine restarted) or 20 (state didn't advance).
//
// Pure C API, no mlua. This tests Eris itself, which is the load-bearing
// question for the entire migration.
#[test]
fn persist_a_suspended_coroutine_and_resume_it() {
    unsafe {
        // === Phase 1: original state ===
        let L = new_lua_state_with_helpers();
        run_script(
            L,
            r#"
            -- Coroutine that yields 10, 20, 30 from deeply-nested calls.
            local function inner(value) coroutine.yield(value) end
            local function middle(value) inner(value) end
            co = coroutine.create(function()
                middle(10)
                middle(20)
                middle(30)
            end)
            local ok1, v1 = coroutine.resume(co)
            assert(ok1 and v1 == 10, "first yield should be 10, got " .. tostring(v1))
            local ok2, v2 = coroutine.resume(co)
            assert(ok2 and v2 == 20, "second yield should be 20, got " .. tostring(v2))
            assert(coroutine.status(co) == "suspended",
                "coroutine should be suspended, got " .. coroutine.status(co))
            "#,
        );

        with_cstr("co", |p| lua_getglobal(L, p));
        assert_eq!(lua_type(L, -1), LUA_TTHREAD, "expected co to be a thread");

        let blob = persist_top(L);
        eprintln!("suspended coroutine serialized to {} bytes", blob.len());
        assert!(!blob.is_empty(), "serialized blob should be non-empty");

        lua_close(L);

        // === Phase 2: fresh state, restore, resume ===
        let L2 = new_lua_state_with_helpers();
        unpersist_to_top(L2, &blob);
        assert_eq!(
            lua_type(L2, -1),
            LUA_TTHREAD,
            "unpersisted value should be a thread"
        );
        with_cstr("co", |p| lua_setglobal(L2, p));

        run_script(
            L2,
            r#"
            assert(coroutine.status(co) == "suspended",
                "restored coroutine should be suspended, got " .. coroutine.status(co))
            -- Resume #3 — should yield 30 from middle(30)→inner→yield.
            local ok, v = coroutine.resume(co)
            assert(ok, "resume #3 failed: " .. tostring(v))
            assert(v == 30,
                "restored coroutine should yield 30, got " .. tostring(v))
            -- After resume #3 we're still suspended INSIDE inner. One
            -- more resume returns through inner→middle→outer and ends.
            local ok4 = coroutine.resume(co)
            assert(ok4, "resume #4 (final) failed")
            assert(coroutine.status(co) == "dead",
                "after final resume coroutine should be dead, got " .. coroutine.status(co))
            "#,
        );

        lua_close(L2);
    }
}

// ============================================================
// Test 4: Closure with upvalues across persist/unpersist
// ============================================================
#[test]
fn persist_closure_with_upvalues() {
    unsafe {
        let L = new_lua_state_with_helpers();
        run_script(
            L,
            r#"
            -- Closure that captures a counter. The upvalue must
            -- be preserved across persist.
            local function make_counter(start)
                local n = start
                return function()
                    n = n + 1
                    return n
                end
            end
            counter = make_counter(100)
            assert(counter() == 101)
            assert(counter() == 102)
            -- Counter's upvalue n is at 102; next call returns 103.
            "#,
        );

        with_cstr("counter", |p| lua_getglobal(L, p));
        let blob = persist_top(L);
        eprintln!("closure serialized to {} bytes", blob.len());
        lua_close(L);

        let L2 = new_lua_state_with_helpers();
        unpersist_to_top(L2, &blob);
        with_cstr("counter", |p| lua_setglobal(L2, p));

        run_script(
            L2,
            r#"
            local v = counter()
            assert(v == 103, "restored counter should return 103, got " .. tostring(v))
            assert(counter() == 104)
            "#,
        );

        lua_close(L2);
    }
}
