use *;

/// Context for some callbacks.
pub struct JackCallbackContext {
    nframes: jack_nframes_t
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
}

impl<F> JackHandler for F where F: FnMut(&JackCallbackContext) -> JackControl + Send + 'static {
    fn process(&mut self, ctx: &JackCallbackContext) -> JackControl {
        self(ctx)
    }
}

extern "C" fn process_callback<T>(nframes: jack_nframes_t, user: *mut libc::c_void) -> i32 where T: JackHandler {
    unsafe {
        let callbacks = &mut *(user as *mut T);
        let ctx = JackCallbackContext {
            nframes: nframes
        };
        callbacks.process(&ctx) as i32
    }
}

pub fn set_handler<F>(conn: &mut JackConnection<Deactivated>, handler: F) -> JackResult<()> where F: JackHandler {
    let user_ptr = Box::into_raw(Box::new(handler));
    let user_ptr = user_ptr as *mut libc::c_void;
    let code = unsafe {
        jack_set_process_callback(conn.handle, Some(process_callback::<F>), user_ptr)
    };
    if code != 0 {
        Err(ErrorKind::UnknownErrorCode("set_process_callback()", code))?
    }
    else {
        Ok(())
    }
}
