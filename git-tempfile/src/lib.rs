//! git-style registered tempfiles that are removed upon typical termination signals.
//!
//! This crate installs signal handlers the first time its facilities are used.
//! These are powered by [`signal-hook`] to get notified when the application is told to shut down
//! using signals to assure these are deleted. The deletion is filtered by process id to allow forks to have their own
//! set of tempfiles that won't get deleted when the parent process exits.
//!
//! As typical handlers for `TERMination` are installed on first use and effectively overriding the defaults, we install
//! default handlers to restore this behaviour. Whether or not to do that can be controlled using [`force_setup()`].
//!
//! # Note
//!
//! Applications setting their own signal handlers on termination to abort the process probably want to be called after the ones of this crate
//! can call [`force_setup()`] before installing their own handlers.
//! By default, our signal handlers will emulate the default behaviour and abort the process after cleaning temporary files.
//!
//! # Limitations
//!
//! ## Tempfiles might remain on disk
//!
//! * Uninterruptible signals are received like `SIGKILL`
//! * The application is performing a write operation on the tempfile when a signal arrives, preventing this tempfile to be removed,
//!   but not others. Any other operation dealing with the tempfile suffers from the same issue.
//!
//! [signal-hook]: https://docs.rs/signal-hook
#![deny(missing_docs, unsafe_code, rust_2018_idioms)]

use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{io, path::Path, sync::atomic::AtomicUsize};
use tempfile::NamedTempFile;

pub mod create_dir;
mod handler;
mod registration;

static SIGNAL_HANDLER_MODE: AtomicUsize = AtomicUsize::new(SignalHandlerMode::default() as usize);
static NEXT_MAP_INDEX: AtomicUsize = AtomicUsize::new(0);
static REGISTER: Lazy<DashMap<usize, Option<ForksafeTempfile>>> = Lazy::new(|| {
    for sig in signal_hook::consts::TERM_SIGNALS {
        // SAFETY: handlers are considered unsafe because a lot can go wrong. See `cleanup_tempfiles()` for details on safety.
        #[allow(unsafe_code)]
        unsafe {
            #[cfg(not(windows))]
            {
                signal_hook_registry::register_sigaction(*sig, handler::cleanup_tempfiles_nix)
            }
            #[cfg(windows)]
            {
                signal_hook::low_level::register(*sig, handler::cleanup_tempfiles_windows)
            }
        }
        .expect("signals can always be installed");
    }
    DashMap::new()
});

/// Define how our signal handlers act
pub enum SignalHandlerMode {
    /// Delete all remaining registered tempfiles on termination.
    DeleteTempfilesOnTermination = 0,
    /// Delete all remaining registered tempfiles on termination and emulate the default handler behaviour.
    ///
    /// This is the default, which leads to the process to be aborted.
    DeleteTempfilesOnTerminationAndRestoreDefaultBehaviour = 1,
}

impl SignalHandlerMode {
    /// By default we will emulate the default behaviour and abort the process.
    ///
    /// While testing, we will not abort the process.
    const fn default() -> Self {
        #[cfg(not(test))]
        return SignalHandlerMode::DeleteTempfilesOnTerminationAndRestoreDefaultBehaviour;
        #[cfg(test)]
        return SignalHandlerMode::DeleteTempfilesOnTermination;
    }
}

/// # Note
///
/// Signals interrupting the calling thread right after taking ownership of the registered tempfile
/// will cause all but this tempfile to be removed automatically. In the common case it will persist on disk as destructors
/// were not called or didn't get to remove the file.
///
/// In the best case the file is a true temporary with a non-clashing name that 'only' fills up the disk,
/// in the worst case the temporary file is used as a lock file which may leave the repository in a locked
/// state forever.
///
/// This kind of raciness exists whenever [`take()`][Registration::take()] is used and can't be circumvented.
pub struct Registration {
    id: usize,
}

struct ForksafeTempfile {
    inner: NamedTempFile,
    owning_process_id: u32,
}

impl From<NamedTempFile> for ForksafeTempfile {
    fn from(inner: NamedTempFile) -> Self {
        ForksafeTempfile {
            inner,
            owning_process_id: std::process::id(),
        }
    }
}

/// A shortcut to [`Registration::new()`].
pub fn new(containing_directory: impl AsRef<Path>) -> io::Result<Registration> {
    Registration::new(containing_directory)
}

/// A shortcut to [`Registration::at_path()`].
pub fn at_path(path: impl AsRef<Path>) -> io::Result<Registration> {
    Registration::at_path(path)
}

/// Explicitly (instead of lazily) initialize signal handlers and other state to keep track of tempfiles.
/// Only has an effect the first time it is called and furthermore allows to set the `mode` in which signal handlers
/// are installed.
///
/// This is required if the application wants to install their own signal handlers _after_ the ones defined here.
pub fn force_setup(mode: SignalHandlerMode) {
    SIGNAL_HANDLER_MODE.store(mode as usize, std::sync::atomic::Ordering::Relaxed);
    Lazy::force(&REGISTER);
}
