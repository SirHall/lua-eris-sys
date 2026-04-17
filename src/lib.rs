//! Low-level FFI bindings to Lua 5.3.5 + Eris.
//!
//! This crate bundles [fnuecke/eris](https://github.com/fnuecke/eris)'s
//! Lua 5.3.5 distribution with Eris's `eris_persist`/`eris_unpersist`
//! added. It compiles into a single static library `liblua-eris.a` and
//! exposes the raw C API via `extern "C"` declarations.
//!
//! # What Eris does
//!
//! Eris walks the full Lua VM state — tables, closures, upvalues,
//! suspended coroutine call stacks — and serializes it to an opaque
//! byte string. `eris_unpersist` restores the serialized state into a
//! fresh Lua VM. Roundtrips preserve all mutable identity and cycles.
//!
//! # Not for direct use
//!
//! This crate exposes raw FFI. For safe usage, see `mlua-eris` (a thin
//! wrapper layer) or call the standard Lua C API via whatever Rust
//! binding you prefer, paired with the persist/unpersist functions.

#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]
#![allow(clippy::missing_safety_doc)]

use std::os::raw::{c_char, c_int, c_void};

/// Opaque handle to a Lua VM state.
#[repr(C)]
pub struct lua_State {
    _private: [u8; 0],
}

/// Integer type used by the Lua VM (64-bit by default in Lua 5.3).
pub type lua_Integer = i64;
/// Number (float) type used by Lua (double by default).
pub type lua_Number = f64;
/// Function pointer type for C functions callable from Lua.
pub type lua_CFunction = unsafe extern "C" fn(L: *mut lua_State) -> c_int;
/// Reader callback used by `lua_load` and `eris_unpersist`.
pub type lua_Reader =
    unsafe extern "C" fn(L: *mut lua_State, ud: *mut c_void, sz: *mut usize) -> *const c_char;
/// Writer callback used by `lua_dump` and `eris_persist`.
pub type lua_Writer =
    unsafe extern "C" fn(L: *mut lua_State, p: *const c_void, sz: usize, ud: *mut c_void) -> c_int;

// === Basic stack indices and constants ===

/// Pseudo-index for the registry.
pub const LUA_REGISTRYINDEX: c_int = -1_001_000;

/// Lua error codes returned by protected calls.
pub const LUA_OK: c_int = 0;
pub const LUA_YIELD: c_int = 1;
pub const LUA_ERRRUN: c_int = 2;
pub const LUA_ERRSYNTAX: c_int = 3;
pub const LUA_ERRMEM: c_int = 4;
pub const LUA_ERRGCMM: c_int = 5;
pub const LUA_ERRERR: c_int = 6;

/// Multiple-return sentinel.
pub const LUA_MULTRET: c_int = -1;

// Lua type codes.
pub const LUA_TNONE: c_int = -1;
pub const LUA_TNIL: c_int = 0;
pub const LUA_TBOOLEAN: c_int = 1;
pub const LUA_TLIGHTUSERDATA: c_int = 2;
pub const LUA_TNUMBER: c_int = 3;
pub const LUA_TSTRING: c_int = 4;
pub const LUA_TTABLE: c_int = 5;
pub const LUA_TFUNCTION: c_int = 6;
pub const LUA_TUSERDATA: c_int = 7;
pub const LUA_TTHREAD: c_int = 8;

// === Core Lua C API — the subset we need for Phase 0 tests ===
// Full bindings can be added later as mlua-eris matures. For now, this
// is enough to open a state, run a script, push/pop a coroutine, and
// invoke Eris.

extern "C" {
    pub fn luaL_newstate() -> *mut lua_State;
    pub fn lua_close(L: *mut lua_State);
    pub fn luaL_openlibs(L: *mut lua_State);

    pub fn luaL_loadstring(L: *mut lua_State, s: *const c_char) -> c_int;
    pub fn lua_pcallk(
        L: *mut lua_State,
        nargs: c_int,
        nresults: c_int,
        msgh: c_int,
        ctx: isize,
        k: Option<lua_CFunction>,
    ) -> c_int;

    pub fn lua_gettop(L: *mut lua_State) -> c_int;
    pub fn lua_settop(L: *mut lua_State, idx: c_int);
    pub fn lua_type(L: *mut lua_State, idx: c_int) -> c_int;
    pub fn lua_pushvalue(L: *mut lua_State, idx: c_int);
    pub fn lua_pushnil(L: *mut lua_State);
    pub fn lua_pushstring(L: *mut lua_State, s: *const c_char) -> *const c_char;
    pub fn lua_pushinteger(L: *mut lua_State, n: lua_Integer);
    pub fn lua_pushboolean(L: *mut lua_State, b: c_int);
    pub fn lua_pushcclosure(L: *mut lua_State, f: lua_CFunction, n: c_int);

    pub fn lua_tolstring(L: *mut lua_State, idx: c_int, len: *mut usize) -> *const c_char;
    pub fn lua_tointegerx(L: *mut lua_State, idx: c_int, isnum: *mut c_int) -> lua_Integer;
    pub fn lua_toboolean(L: *mut lua_State, idx: c_int) -> c_int;

    pub fn lua_getglobal(L: *mut lua_State, name: *const c_char) -> c_int;
    pub fn lua_setglobal(L: *mut lua_State, name: *const c_char);

    pub fn lua_createtable(L: *mut lua_State, narr: c_int, nrec: c_int);
    pub fn lua_rawseti(L: *mut lua_State, idx: c_int, n: lua_Integer);
    pub fn lua_rawset(L: *mut lua_State, idx: c_int);

    // Coroutine / thread API
    pub fn lua_newthread(L: *mut lua_State) -> *mut lua_State;
    pub fn lua_resume(L: *mut lua_State, from: *mut lua_State, narg: c_int) -> c_int;

    pub fn lua_error(L: *mut lua_State) -> c_int;

    // === Eris persistence API ===
    // See eris.h in the bundled source. Both functions take value and
    // perms indices relative to the current Lua stack.
    //
    // `eris_persist(L, perms_idx, value_idx)` — pops nothing; pushes
    // a Lua string containing the serialized state onto the stack.
    // Raises a Lua error (via longjmp) on failure.
    //
    // `eris_unpersist(L, perms_idx, str_idx)` — pops nothing; pushes
    // the reconstructed Lua value onto the stack. Raises on failure.
    pub fn eris_persist(L: *mut lua_State, perms_idx: c_int, value_idx: c_int);
    pub fn eris_unpersist(L: *mut lua_State, perms_idx: c_int, str_idx: c_int);

    // Eris settings — optional, useful for debug.
    // `eris_set_setting(L, name_idx, value_idx)` — pops 2 args (name, value)
    // `eris_get_setting(L, name_idx)` — pops 1, pushes the current value
    pub fn eris_set_setting(L: *mut lua_State, name_idx: c_int, value_idx: c_int);
    pub fn eris_get_setting(L: *mut lua_State, name_idx: c_int);
}

// === Convenience inline wrappers for common macro-like helpers ===
// The Lua C API exposes several helpers as macros; we replicate them
// as inline functions so Rust code can call them.

/// Pop `n` values from the stack.
#[inline]
pub unsafe fn lua_pop(L: *mut lua_State, n: c_int) {
    lua_settop(L, -n - 1);
}

/// Equivalent to `lua_tolstring(L, idx, NULL)` — fetch string without length.
#[inline]
pub unsafe fn lua_tostring(L: *mut lua_State, idx: c_int) -> *const c_char {
    lua_tolstring(L, idx, std::ptr::null_mut())
}

/// `lua_pcall(L, nargs, nresults, msgh)` — the non-continuation variant.
#[inline]
pub unsafe fn lua_pcall(
    L: *mut lua_State,
    nargs: c_int,
    nresults: c_int,
    msgh: c_int,
) -> c_int {
    lua_pcallk(L, nargs, nresults, msgh, 0, None)
}
