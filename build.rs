//! Compile Lua 5.3.5 + Eris into a single static library.
//!
//! The bundled source is fnuecke/eris master-lua5.3 branch as of 2017
//! (commit 376bc5d), which is Lua 5.3.5 with eris.c/eris.h added and
//! minor patches. See lua-eris-5.3.5/LICENSE.lua-eris for upstream
//! license information.

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_dir = manifest_dir.join("lua-eris-5.3.5");

    // All .c files in the bundled tree EXCEPT the two standalone binary
    // entry points (lua.c = the interpreter `lua`, luac.c = the bytecode
    // compiler `luac`). Everything else is library code.
    let library_sources = [
        "eris.c",       // Eris persistence library
        "lapi.c",       // Lua C API
        "lauxlib.c",    // Auxiliary API
        "lbaselib.c",   // Base library (print, type, etc.)
        "lbitlib.c",    // Bit operations library
        "lcode.c",      // Bytecode generator
        "lcorolib.c",   // Coroutine library
        "lctype.c",     // Character type helpers
        "ldblib.c",     // Debug library
        "ldebug.c",     // Debug interface
        "ldo.c",        // Stack + function call handling
        "ldump.c",      // Bytecode serialization (separate from Eris)
        "lfunc.c",      // Function prototype + closure handling
        "lgc.c",        // Garbage collector
        "linit.c",      // Standard library opener
        "liolib.c",     // I/O library
        "llex.c",       // Lexer
        "lmathlib.c",   // Math library
        "lmem.c",       // Memory allocator
        "loadlib.c",    // Dynamic library loader
        "lobject.c",    // Object manipulation
        "lopcodes.c",   // Opcode definitions
        "loslib.c",     // OS library
        "lparser.c",    // Parser
        "lstate.c",     // Lua state management
        "lstring.c",    // String interning
        "lstrlib.c",    // String library
        "ltable.c",     // Table implementation
        "ltablib.c",    // Table library
        "ltm.c",        // Metatable / tag method handling
        "lundump.c",    // Bytecode deserialization
        "lutf8lib.c",   // UTF-8 library
        "lvm.c",        // Virtual machine
        "lzio.c",       // Input stream abstraction
    ];

    let mut build = cc::Build::new();

    build.include(&src_dir);

    // Platform-specific defines matching Lua's Makefile. Each of these
    // pulls in the appropriate POSIX/DLOPEN/etc. defines transitively
    // via luaconf.h — don't add them explicitly or we get redefine
    // warnings.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" => {
            build.define("LUA_USE_MACOSX", None);
        }
        "linux" => {
            build.define("LUA_USE_LINUX", None);
        }
        _ => {
            // Windows and others: leave luaconf.h's defaults. This
            // project targets macOS/Linux primarily.
        }
    }

    // Suppress upstream Lua warnings — we don't own this code, we
    // shouldn't gate CI on its style. But keep Rust-side warnings hot.
    build
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-unused-function")
        .flag_if_supported("-Wno-missing-field-initializers")
        // eris.c has a few constant-conditional branches that trip
        // modern clang's stricter checks. Silence; the code is correct.
        .flag_if_supported("-Wno-unused-value")
        .flag_if_supported("-Wno-implicit-fallthrough");

    // Register all .c files.
    for src in &library_sources {
        build.file(src_dir.join(src));
    }

    // Suppress cc's automatic `cargo:rustc-link-lib=static=lua-eris` —
    // we emit our own with the +whole-archive modifier below.
    build.cargo_metadata(false);

    build.compile("lua-eris");

    // On macOS the linker is one-pass — archive members aren't pulled
    // in unless previously-undefined symbols match. mlua-sys references
    // luaopen_coroutine etc. AFTER our archive in the link line, so
    // those symbols would never be resolved without forcing the whole
    // archive. The +whole-archive modifier on rustc-link-lib (stable
    // since Rust 1.61) handles this portably across linkers.
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static:+whole-archive=lua-eris");

    // Tell Cargo to rerun the build if the upstream sources change
    // (e.g., if we pull in bugfix patches to eris.c).
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=lua-eris-5.3.5");
    for src in &library_sources {
        println!(
            "cargo:rerun-if-changed=lua-eris-5.3.5/{}",
            src
        );
    }

    // Export metadata for downstream crates (mlua-eris wrapper) to use
    // via DEP_LUA_ERIS_* environment variables. The include path lets
    // downstream crates find the Lua+Eris headers if they need bindgen.
    // The lib path lets a downstream build.rs point mlua-sys at our
    // pre-built lib (via LUA_LIB / LUA_LIB_NAME env vars).
    //
    // Cargo convention: a `cargo:foo=bar` from a sys crate becomes
    // `DEP_<links>_FOO=bar` in downstream build scripts. Our `links`
    // is "lua-eris", so these become DEP_LUA_ERIS_INCLUDE and DEP_LUA_ERIS_LIB.
    println!("cargo:include={}", src_dir.display());
    if let Ok(out_dir) = env::var("OUT_DIR") {
        println!("cargo:lib={}", out_dir);
        // The actual library file produced by cc::Build is at
        // $OUT_DIR/liblua-eris.a on Unix.
    }
}
