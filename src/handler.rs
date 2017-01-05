//! Callback-based JACK API functions (logging, processing).

use super::{JackNFrames, JackPort, JackStatus, JackConnection, Deactivated};
use jack_sys::*;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::ffi::CStr;
use libc;
use errors::*;

/// An object that can receive info and error messages from JACK.
pub trait JackLoggingHandler: Send {
    /// Called when JACK displays an error message.
    fn on_error(&mut self, msg: &str);
    /// Called when JACK displays an info message.
    fn on_info(&mut self, msg: &str);
}

lazy_static! {
    static ref LOGGING_HANDLER: AtomicPtr<*mut JackLoggingHandler> = AtomicPtr::new(::std::ptr::null_mut());
}
unsafe extern "C" fn error_callback(msg: *const libc::c_char) {
    let handler = LOGGING_HANDLER.load(Ordering::Relaxed);
    if !handler.is_null() {
        let f = &mut (**handler);
        let msg = CStr::from_ptr(msg);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            f.on_error(&msg.to_string_lossy());
        }));
    }
}
unsafe extern "C" fn info_callback(msg: *const libc::c_char) {
    let handler = LOGGING_HANDLER.load(Ordering::Relaxed);
    if !handler.is_null() {
        let f = &mut (**handler);
        let msg = CStr::from_ptr(msg);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            f.on_info(&msg.to_string_lossy());
        }));
    }
}
/// Set the logging handler (a struct that implements `JackLoggingHandler`).
///
/// # Safety
///
/// **Warning:** Your handler will never be deallocated / `Drop`ped.
pub fn set_logging_handler<F>(handler: F) where F: JackLoggingHandler + 'static {
    unsafe {
        let trait_object_ptr = Box::into_raw(Box::new(handler) as Box<JackLoggingHandler>);
        let ptr_ception = Box::into_raw(Box::new(trait_object_ptr));
        LOGGING_HANDLER.store(ptr_ception, Ordering::Relaxed);
        jack_set_error_function(Some(error_callback));
        jack_set_info_function(Some(info_callback));
    }
}
/// Context for some callbacks.
pub struct JackCallbackContext {
    nframes: JackNFrames
}

impl JackCallbackContext {
    /// Returns the number of frames that must be processed in this callback.
    #[inline(always)]
    pub fn nframes(&self) -> u32 {
        self.nframes
    }
    /// Gets the buffer of a port, if the port is valid.
    pub fn get_port_buffer(&self, port: &JackPort) -> Option<&mut [f32]> {
        unsafe {
            let buf = jack_port_get_buffer(port.as_ptr(), self.nframes);
            if buf.is_null() {
                None
            }
            else {
                Some(::std::slice::from_raw_parts_mut(buf as *mut f32, self.nframes as usize))
            }
        }
    }
}

/// Return type of callback functions.
#[derive(Copy, Clone, Debug)]
pub enum JackControl {
    /// Continue processing.
    Continue = 0,
    /// Stop processing.
    Stop = -1
}
/// Trait for an object that implements JACK callbacks.
///
/// Most of the default implementations return `JackControl::Continue` - however, **process() does not** - you
/// must explicitly override this behaviour if you want to specify nothing for `process()`.
pub trait JackHandler: Send {
    /// This function is called by the engine any time there is work to be done.
    /// Return `JackControl::Stop` to stop processing, otherwise return
    /// `JackControl::Continue` to continue.
    ///
    /// # Realtime safety
    ///
    /// The code in the supplied function **MUST** be suitable for real-time execution.
    /// That means that it cannot call functions that might block for a long time.
    /// This includes all I/O functions (disk, TTY, network), malloc, free, printf,
    /// pthread_mutex_lock, sleep, wait, poll, select, pthread_join, pthread_cond_wait, etc.
    ///
    /// Rust-specific things to **avoid** using: `std` MPSC channels (use
    /// [bounded_spsc_queue](https://github.com/polyfractal/bounded-spsc-queue) instead, or
    /// similar ringbuffer solution), mutexes, `RWLock`s, `Barrier`s, `String`s, `Vec`s (use
    /// a **pre-allocated** [ArrayVec](https://github.com/bluss/arrayvec) instead), and
    /// anything under `std::collections`.
    fn process(&mut self, _ctx: &JackCallbackContext) -> JackControl {
        JackControl::Stop
    }
    /// This is called whenever the size of the the buffer that will be passed to the
    /// `process()` function is about to change.
    /// Clients that depend on knowing the buffer size must implement this callback.
    fn buffer_size(&mut self, _new_size: JackNFrames) -> JackControl { JackControl::Continue }
    /// Called whenever the system sample rate changes.
    ///
    /// Given that the JACK API exposes no way to change the sample rate, the library author
    /// would like you to know that this is a decidedly rare occurence. Still, it's worth
    /// being prepared ;)
    fn sample_rate(&mut self, _new_rate: JackNFrames) -> JackControl { JackControl::Continue }
    /// Called just once after the creation of the thread in which all other callbacks are
    /// handled.
    fn thread_init(&mut self) {  }
    /// To be called if and when the JACK server shuts down the client thread.
    fn shutdown(&mut self, _status: JackStatus, _reason: &str) { }
    /// Called whenever a client is registered or unregistered.
    ///
    /// Use the `registered` argument to determine which it is.
    fn client_registered(&mut self, _name: &str, _registered: bool) { }
    /// Called when an XRUN (over- or under- run) occurs.
    fn xrun(&mut self) -> JackControl { JackControl::Continue }
}
/*
    /// Called whenever a port is registered or unregistered.
    ///
    /// Use the `registered` argument to determine which it is.
    fn port_registered(&mut self, _port: JackPort, _registered: bool) { }
    /// Called whenever a port is renamed.
    fn port_renamed(&mut self, _port: JackPort, _old_name: &str, _new_name: &str) { }
    /// Called whenever ports are connected or disconnected.
    ///
    /// Use the `connected` argument to determine which it is.
    fn port_connected(&mut self, _from: JackPort, _to: JackPort, _connected: bool) { }
    /// Called whenever the processing graph is reordered.
    fn graph_reorder(&mut self) -> JackControl { JackControl::Continue }
    /// Called when the JACK server starts or stops freewheeling.
    ///
    /// Use the `freewheel` argument to determine which it is.
    fn freewheel(&mut self, _freewheel: bool) { }
*/

impl<F> JackHandler for F where F: FnMut(&JackCallbackContext) -> JackControl + Send + 'static {
    fn process(&mut self, ctx: &JackCallbackContext) -> JackControl {
        self(ctx)
    }
}
unsafe extern "C" fn buffer_size_callback<T>(frames: JackNFrames, user: *mut libc::c_void) -> libc::c_int where T: JackHandler {
    let callbacks = &mut *(user as *mut T);
    catch_unwind(AssertUnwindSafe(|| {
        callbacks.buffer_size(frames) as _
    })).unwrap_or(-1)
}
unsafe extern "C" fn sample_rate_callback<T>(frames: JackNFrames, user: *mut libc::c_void) -> libc::c_int where T: JackHandler {
    let callbacks = &mut *(user as *mut T);
    catch_unwind(AssertUnwindSafe(|| {
        callbacks.sample_rate(frames) as _
    })).unwrap_or(-1)
}
unsafe extern "C" fn client_registration_callback<T>(name: *const libc::c_char, register: libc::c_int, user: *mut libc::c_void) where T: JackHandler {
    let callbacks = &mut *(user as *mut T);
    let name = CStr::from_ptr(name);
    let _ = catch_unwind(AssertUnwindSafe(|| {
        callbacks.client_registered(&name.to_string_lossy(), register != 0)
    }));

}
unsafe extern "C" fn info_shutdown_callback<T>(code: jack_status_t, reason: *const libc::c_char, user: *mut libc::c_void) where T: JackHandler {
    let callbacks = &mut *(user as *mut T);
    let code = JackStatus::from_bits_truncate(code);
    let reason = CStr::from_ptr(reason);
    let _ = catch_unwind(AssertUnwindSafe(|| {
        callbacks.shutdown(code, &reason.to_string_lossy())
    }));

}
unsafe extern "C" fn thread_init_callback<T>(user: *mut libc::c_void) where T: JackHandler {
    let callbacks = &mut *(user as *mut T);
    let _ = catch_unwind(AssertUnwindSafe(|| {
        callbacks.thread_init()
    }));
}
unsafe extern "C" fn process_callback<T>(nframes: JackNFrames, user: *mut libc::c_void) -> libc::c_int where T: JackHandler {
    let callbacks = &mut *(user as *mut T);
    let ctx = JackCallbackContext {
        nframes: nframes
    };
    catch_unwind(AssertUnwindSafe(|| {
        callbacks.process(&ctx) as _
    })).unwrap_or(-1)
}
unsafe extern "C" fn xrun_callback<T>(user: *mut libc::c_void) -> libc::c_int where T: JackHandler {
    let callbacks = &mut *(user as *mut T);
    catch_unwind(AssertUnwindSafe(|| {
        callbacks.xrun() as _
    })).unwrap_or(-1)
}
pub fn set_handler<F>(conn: &mut JackConnection<Deactivated>, handler: F) -> JackResult<()> where F: JackHandler {
    let user_ptr = Box::into_raw(Box::new(handler));
    let user_ptr = user_ptr as *mut libc::c_void;
    unsafe {
        let code = jack_set_process_callback(conn.handle, Some(process_callback::<F>), user_ptr);
        if code != 0 { Err(ErrorKind::UnknownErrorCode("set_process_callback() - process", code))? }
        let code = jack_set_thread_init_callback(conn.handle, Some(thread_init_callback::<F>), user_ptr);
        if code != 0 { Err(ErrorKind::UnknownErrorCode("set_process_callback() - thread_init", code))? }
        let code = jack_set_buffer_size_callback(conn.handle, Some(buffer_size_callback::<F>), user_ptr);
        if code != 0 { Err(ErrorKind::UnknownErrorCode("set_process_callback() - buffer_size", code))? }
        let code = jack_set_sample_rate_callback(conn.handle, Some(sample_rate_callback::<F>), user_ptr);
        if code != 0 { Err(ErrorKind::UnknownErrorCode("set_process_callback() - sample_rate", code))? }
        let code = jack_set_xrun_callback(conn.handle, Some(xrun_callback::<F>), user_ptr);
        if code != 0 { Err(ErrorKind::UnknownErrorCode("set_process_callback() - xrun", code))? }
        let code = jack_set_client_registration_callback(conn.handle, Some(client_registration_callback::<F>), user_ptr);
        if code != 0 { Err(ErrorKind::UnknownErrorCode("set_process_callback() - client_registration", code))? }
        jack_on_info_shutdown(conn.handle, Some(info_shutdown_callback::<F>), user_ptr);
    }
    Ok(())
}
