// SPDX-License-Identifier: GPL-3.0-only

//! `mind-headless` — thin binary over the `mind_headless` library.
//!
//! Plan 22 reuses the library as the dedicated `server` mode (§12 C8).

fn main() -> std::process::ExitCode {
    // Windows sizes the main thread stack at 1 MiB, which is not enough for the
    // deeply-nested `bevy_ecs::World` teardown some fixtures perform (they pass
    // on Linux's 8 MiB default). Run the oracle on a thread with a generous
    // stack so the headless harness behaves identically cross-platform.
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let code = std::thread::Builder::new()
        .name("mind-headless".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || mind_headless::run_from_args(args))
        .map(|handle| handle.join().unwrap_or(1))
        .unwrap_or(1);
    std::process::ExitCode::from(code as u8)
}
