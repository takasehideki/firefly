//#![deny(warnings)]
#![allow(stable_features)]
// `rand` has link errors
#![allow(broken_intra_doc_links)]
// For allocating multiple contiguous terms, like for Tuples.
#![feature(allocator_api)]
#![feature(backtrace)]
#![feature(bind_by_move_pattern_guards)]
#![feature(exact_size_is_empty)]
// For `lumen_otp::erlang::term_to_binary`
#![feature(float_to_from_bytes)]
#![feature(fn_traits)]
// For `crate::reference::count
#![feature(integer_atomics)]
// For `crate::binary::heap::<Iter as Iterator>.size_hint`
#![feature(ptr_offset_from)]
// For allocation multiple contiguous terms in `Term::alloc_count`.
#![feature(try_reserve)]
#![feature(type_ascription)]
// for `crate::term::Term`
#![feature(untagged_unions)]
// `crate::registry::<Registered as PartialEq>::eq`
#![feature(weak_ptr_eq)]
// Layout helpers
#![feature(alloc_layout_extra)]
// liblumen_core::entry
#![feature(termination_trait_lib)]
#![feature(process_exitcode_placeholder)]
// `PROCESS_SIGNAL`
#![feature(thread_local)]
// Unwinding across C ABI
#![feature(c_unwind)]

extern crate alloc;
extern crate cfg_if;
extern crate chrono;

use anyhow::anyhow;

pub use lumen_rt_core::{
    base, binary_to_string, context, distribution, integer_to_string, proplist, registry, send,
    test, time, timer,
};

#[cfg(not(any(test, target_arch = "wasm32")))]
mod config;
pub mod future;
mod logging;
pub mod process;
// `pub` for `examples/spawn-chain`
pub mod scheduler;
// `pub` for `examples/spawn-chain`
pub mod sys;
// `pub` for `examples/spawn-chain`
mod term;

/// The main entry point for the runtime
///
/// NOTE: This is currently conditionally compiled, since `lumen_web` depends on
/// `lumen_rt_full`, and we only can define `#[entry]` once in a dependency tree.
/// We will need to split up `lumen_rt_full` into smaller components eventually,
/// but for now it is sufficient to just conditionally compile the entry point here
#[cfg(not(any(test, target_arch = "wasm32")))]
#[liblumen_core::entry]
fn main() -> i32 {
    use std::process::Termination;

    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");
    main_internal(name, version, std::env::args().collect())
        .report()
        .to_i32()
}

#[cfg(not(any(test, target_arch = "wasm32")))]
fn main_internal(name: &str, version: &str, argv: Vec<String>) -> anyhow::Result<()> {
    use self::config::Config;
    use self::logging::Logger;
    use self::sys::break_handler::{self, Signal};
    use bus::Bus;
    use log::Level;
    use std::thread;

    // Load system configuration
    let _config = match Config::from_argv(name.to_string(), version.to_string(), argv) {
        Ok(config) => config,
        Err(err) => {
            return Err(anyhow!(err));
        }
    };

    // This bus is used to receive signals across threads in the system
    let mut bus: Bus<break_handler::Signal> = Bus::new(1);
    // Each thread needs a reader
    let mut rx1 = bus.add_rx();
    // Initialize the break handler with the bus, which will broadcast on it
    break_handler::init(bus);

    // Start logger
    Logger::init(Level::Info).expect("Unexpected failure initializing logger");

    let scheduler = scheduler::current();
    loop {
        // Run the scheduler for a cycle
        let scheduled = scheduler.run_once();
        // Check for system signals, and terminate if needed
        if let Ok(sig) = rx1.try_recv() {
            match sig {
                // For now, SIGINT initiates a controlled shutdown
                Signal::INT => {
                    // If an error occurs, report it before shutdown
                    if let Err(err) = scheduler.shutdown() {
                        return Err(anyhow!(err));
                    } else {
                        break;
                    }
                }
                // Technically, we may never see these signals directly,
                // we may just be terminated out of hand; but just in case,
                // we handle them explicitly by immediately terminating, so
                // that we are good citizens of the operating system
                sig if sig.should_terminate() => {
                    return Ok(());
                }
                // All other signals can be surfaced to other parts of the
                // system for custom use, e.g. SIGCHLD, SIGALRM, SIGUSR1/2
                _ => (),
            }
        }
        // If the scheduler scheduled a process this cycle, then we're busy
        // and should keep working until we have an idle period
        if scheduled {
            continue;
        }
        // Otherwise,
        // In some configurations, it makes more sense for us to spin and use
        // spin_loop_hint here instead; namely when we're supposed to be the primary
        // software on a system, and threads are pinned to cores, it makes no sense
        // to yield to the system scheduler. However on a system with contention for
        // system resources, or where threads aren't pinned to cores, we're better off
        // explicitly yielding to the scheduler, rather than waiting to be preempted at
        // a potentially inopportune time.
        //
        // In any case, for now, we always explicitly yield until we've got proper support
        // for configuring the system
        thread::yield_now()
    }

    Ok(())
}
