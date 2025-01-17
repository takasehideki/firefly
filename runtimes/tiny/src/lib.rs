#![feature(c_unwind)]
#![feature(once_cell)]
#![feature(ptr_metadata)]
#![feature(process_exitcode_internals)]
#![feature(thread_local)]
#![feature(let_else)]
#![feature(iterator_try_collect)]

extern crate firefly_crt;

mod env;
mod erlang;
mod init;
mod intrinsic;
mod scheduler;
mod sys;

use bus::Bus;
use std::process::ExitCode;

use self::sys::break_handler::{self, Signal};

#[export_name = "firefly_entry"]
pub unsafe extern "C" fn main() -> i32 {
    use std::process::Termination;

    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");
    main_internal(name, version, vec![]).report().to_i32()
}

fn main_internal(_name: &str, _version: &str, _argv: Vec<String>) -> ExitCode {
    self::env::init(std::env::args_os()).unwrap();

    // This bus is used to receive signals across threads in the system
    let mut bus: Bus<Signal> = Bus::new(1);
    // Each thread needs a reader
    let mut rx1 = bus.add_rx();
    // Initialize the break handler with the bus, which will broadcast on it
    break_handler::init(bus);

    scheduler::init();
    scheduler::with_current(|scheduler| scheduler.spawn_init()).unwrap();
    loop {
        // Run the scheduler for a cycle
        let scheduled = scheduler::with_current(|scheduler| scheduler.run_once());
        // Check for system signals, and terminate if needed
        if let Ok(sig) = rx1.try_recv() {
            match sig {
                // For now, SIGINT initiates a controlled shutdown
                Signal::INT => {
                    // If an error occurs, report it before shutdown
                    break;
                }
                // Technically, we may never see these signals directly,
                // we may just be terminated out of hand; but just in case,
                // we handle them explicitly by immediately terminating, so
                // that we are good citizens of the operating system
                sig if sig.should_terminate() => {
                    return ExitCode::FAILURE;
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

        break;
    }

    scheduler::with_current(|s| s.shutdown())
}
