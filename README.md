# lua-eris-sys

Bundles **Lua 5.3.5 + [Eris](https://github.com/fnuecke/eris)** (Florian Nuecke's persistence library) and compiles them into a single static library `liblua-eris.a`. Provides minimal Rust FFI bindings for the subset of the Lua C API + Eris's `eris_persist`/`eris_unpersist` needed for testing.

**You probably don't want to use this crate directly.** Use [`mlua-eris`](../mlua-eris) instead — it provides a safe API on top of `mlua` that handles perms-table management, error wrapping, and the C-function loader.

## What's bundled

- **Lua 5.3.5** — full source from Lua.org
- **Eris** — Florian Nuecke's coroutine/state persistence library, integrated into Lua's `linit.c` so it auto-loads as global `eris` when `luaL_openlibs` runs

Source comes from [`fnuecke/eris`](https://github.com/fnuecke/eris) `master-lua5.3` branch (commit `376bc5d`, 2017). License attribution in `lua-eris-5.3.5/LICENSE.lua-eris`.

## Why not Lua 5.4?

Eris hasn't been forward-ported to 5.4. The closest community fork (MovingBlocks/eris @ master-lua5.4) is half-finished with TODOs in the coroutine persistence path. Lua 5.3 is still receiving security patches and is the production-ready target for Eris-based persistence.

## Build

```bash
cargo build              # builds liblua-eris.a in OUT_DIR
cargo test               # runs FFI roundtrip tests
```

The build script uses `cc` to compile all 34 .c files into a single archive. macOS and Linux supported (`LUA_USE_MACOSX`, `LUA_USE_LINUX`). Windows would need a build.rs adjustment.

## Linking from another crate

`lua-eris-sys`'s build script emits:

```
cargo:rustc-link-search=native=$OUT_DIR
cargo:rustc-link-lib=static:+whole-archive=lua-eris
```

The `+whole-archive` modifier is critical — macOS's linker is one-pass, so without forcing the whole archive, symbols not yet referenced when the archive is processed get dropped. **Note:** Cargo also drops link directives for crates whose symbols nothing references in the final binary. To keep cargo from treating `lua-eris-sys` as rmeta-only, add this somewhere in your downstream crate:

```rust
#[allow(unused_imports)]
use lua_eris_sys::eris_persist as _force_link;
```

## API surface

The exposed FFI is a deliberately minimal subset of the Lua C API plus Eris's two entry points:

```rust
pub fn eris_persist(L: *mut lua_State, perms_idx: c_int, value_idx: c_int);
pub fn eris_unpersist(L: *mut lua_State, perms_idx: c_int, str_idx: c_int);
```

Plus enough of the standard Lua C API (`luaL_newstate`, `luaL_openlibs`, `lua_pcall`, etc.) to write integration tests. Full bindings are NOT a goal — use `mlua-sys` paired with this crate's `external` feature for that.

## License

MIT (matching both Lua and Eris).
