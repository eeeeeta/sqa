#[macro_use]
extern crate bitflags;
extern crate libc;
extern crate jack_sys;
#[macro_use]
extern crate error_chain;

static JACK_DEFAULT_AUDIO_TYPE: &'static [u8] = b"32 bit float mono audio\0";
pub mod errors;
pub mod handler;
pub mod port;

use jack_sys::*;
use std::ffi::{CString, CStr};
use std::marker::PhantomData;
use std::borrow::Cow;
use errors::{ErrorKind, ChainErr};
pub use errors::JackResult;
pub use handler::{JackCallbackContext, JackControl, JackHandler};
pub use port::JackPort;
pub use jack_sys::{jack_nframes_t, jack_port_t};
bitflags! {
    /// Status of an operation.
    ///
    /// See `STATUS_*` constants for possible values.
    pub flags JackStatus: libc::c_uint {
        /// Overall operation failed.
        const STATUS_FAILURE = jack_sys::JackFailure,
        /// The operation contained an invalid or unsupported option.
        const STATUS_INVALID_OPTION = jack_sys::JackInvalidOption,
        /// The desired client name was not unique. With the JackUseExactName option this
        /// situation is fatal. Otherwise, the name was modified by appending a dash and a
        /// two-digit number in the range "-01" to "-99". The jack_get_client_name()
        /// function will return the exact string that was used. If the specified
        /// client_name plus these extra characters would be too long, the open fails
        /// instead.
        const STATUS_NAME_NOT_UNIQUE = jack_sys::JackNameNotUnique,
        /// The JACK server was started as a result of this operation. Otherwise,
        /// it was running already. In either case the caller is now connected to jackd,
        /// so there is no race condition. When the server shuts down, the client will find
        /// out.
        const STATUS_SERVER_STARTED = jack_sys::JackServerStarted,
        /// Unable to connect to the JACK server.
        const STATUS_SERVER_FAILED = jack_sys::JackServerFailed,
        /// Communication error with the JACK server.
        const STATUS_SERVER_ERROR = jack_sys::JackServerError,
        /// Requested client does not exist.
        const STATUS_NO_SUCH_CLIENT = jack_sys::JackNoSuchClient,
        /// Unable to load internal client.
        const STATUS_LOAD_FAILURE = jack_sys::JackLoadFailure,
        /// Unable to initialise client.
        const STATUS_INIT_FAILURE = jack_sys::JackInitFailure,
        /// Unable to access shared memory.
        const STATUS_SHM_FAILURE = jack_sys::JackShmFailure,
        /// Client's protocol version does not match.
        const STATUS_VERSION_ERROR = jack_sys::JackShmFailure
    }
}
bitflags! {
    /// A port has a set of flags that are formed by OR-ing together the desired values
    /// from the consts `PORT_*`. The flags "JackPortIsInput" and "JackPortIsOutput" are
    /// mutually exclusive and it is an error to use them both.
    pub flags JackPortFlags: libc::c_ulong {
        /// This port can receive data.
        const PORT_IS_INPUT = JackPortIsInput as libc::c_ulong,
        /// Data can be read from this port.
        const PORT_IS_OUTPUT = JackPortIsOutput as libc::c_ulong,
        /// The port corresponds to some kind of physical I/O connector.
        const PORT_IS_PHYSICAL = JackPortIsPhysical as libc::c_ulong,
        /// A call to jack_port_request_monitor() makes sense.
        ///
        /// Precisely what this means is dependent on the client. A typical result of it
        /// being called with TRUE as the second argument is that data that would be
        /// available from an output port (with JackPortIsPhysical set) is sent to a
        /// physical output connector as well, so that it can be heard/seen/whatever.
        ///
        /// Clients that do not control physical interfaces should never create ports with
        /// this bit set.
        const PORT_CAN_MONITOR = JackPortCanMonitor as libc::c_ulong,
        /// For an input port: the data received by this port will not be passed on or
        /// made available at any other port.
        ///
        /// For an output port: the data available at the port does not originate from any
        /// other port.
        ///
        /// Audio synthesizers, I/O hardware interface clients, HDR systems are examples
        /// of clients that would set this flag for their ports.
        const PORT_IS_TERMINAL = JackPortIsTerminal as libc::c_ulong
    }
}
bitflags! {
    /// Options for opening a connection to JACK, formed by OR-ing together desired values
    /// from the consts `OPEN_*`.
    pub flags JackOpenOptions: libc::c_uint {
        /// Do not automatically start the JACK server when it is not already running.
        /// This option is always selected if $JACK_NO_START_SERVER is defined in the
        /// calling process environment.
        const OPEN_NO_START_SERVER = JackNoStartServer,
        /// Use the exact client name requested. Otherwise, JACK automatically generates
        /// a unique one, if needed.
        const OPEN_USE_EXACT_NAME = JackUseExactName,
    }
}
/// Type argument for deactivated connections.
pub struct Deactivated;
/// Type argument for activated connections.
pub struct Activated;
/// A connection to a JACK server (known as a "client" in the JACK docs).
///
/// Exists in two types: `JackConnection<Activated>` when `activate()` has been called
/// (i.e. audio is being processed), and `<Deactivated>` when this has not happened,
/// or `deactivate()` has been called.
pub struct JackConnection<T> {
    handle: *mut jack_client_t,
    sample_rate: u32,
    _phantom: PhantomData<T>
}

/// Helper function to convert Rust `&str`s to `CString`s.
fn str_to_cstr(st: &str) -> JackResult<CString> {
    Ok(CString::new(st).chain_err(|| ErrorKind::NulError)?)
}

impl<T> JackConnection<T> {
    pub fn as_ptr(&self) -> *const jack_client_t {
        self.handle
    }
    /// Get the sample rate of the JACK server.
    pub fn sample_rate(&self) -> jack_nframes_t {
        self.sample_rate
    }
    /// Get the CPU load of the JACK server.
    pub fn cpu_load(&self) -> libc::c_float {
        unsafe {
            jack_cpu_load(self.handle)
        }
    }
    /// Get the buffer size passed to the `process()` callback.
    pub fn buffer_size(&self) -> jack_nframes_t {
        unsafe {
            jack_get_buffer_size(self.handle)
        }
    }
    /// Change the buffer size passed to the `process()` callback.
    ///
    /// This operation **stops the JACK engine process cycle**, then calls all registered
    /// bufsize_callback functions before restarting the process cycle. This will cause a
    /// gap in the audio flow, so it should only be done at appropriate stopping points.
    ///
    /// # Parameters
    ///
    /// - bufsize: new buffer size. Must be a power of two.
    ///
    /// # Errors
    ///
    /// - `NotPowerOfTwo`: if the new buffer size isn't a power of two
    /// - `UnknownErrorCode`
    pub fn set_buffer_size(&mut self, bufsize: jack_nframes_t) -> JackResult<()> {
        if bufsize.next_power_of_two() != bufsize {
            Err(ErrorKind::NotPowerOfTwo)?
        }
        let code = unsafe {
            jack_set_buffer_size(self.handle, bufsize)
        };
        if code != 0 {
            Err(ErrorKind::UnknownErrorCode("set_buffer_size()", code))?
        }
        else {
            Ok(())
        }
    }
    unsafe fn activate_or_deactivate<X>(self, activate: bool) -> Result<JackConnection<X>, (Self, errors::Error)> {
        let code = {
            if activate {
                jack_activate(self.handle)
            }
            else {
                jack_deactivate(self.handle)
            }
        };
        if code != 0 {
            Err((self, ErrorKind::UnknownErrorCode("activate_or_deactivate()", code).into()))
        }
        else {
            Ok(::std::mem::transmute::<JackConnection<T>, JackConnection<X>>(self))
        }
    }
    fn connect_or_disconnect_ports(&mut self, from: &JackPort, to: &JackPort, conn: bool) -> JackResult<()> {
        if from.get_type()? != to.get_type()? {
            Err(ErrorKind::InvalidPortType)?;
        }
        if !from.get_flags().contains(PORT_IS_OUTPUT) || !to.get_flags().contains(PORT_IS_INPUT) {
            Err(ErrorKind::InvalidPortFlags)?;
        }
        let code = unsafe {
            if conn {
                jack_connect(self.handle, from.get_name_raw(false)?, to.get_name_raw(false)?)
            }
            else {
                jack_disconnect(self.handle, from.get_name_raw(false)?, to.get_name_raw(false)?)
            }
        };
        match code {
            47 => Ok(()),
            0 => Ok(()),
            _ => Err(ErrorKind::UnknownErrorCode("connect_or_disconnect_ports()", code))?
        }
    }
    /// Establish a connection between two ports.
    ///
    /// When a connection exists, data written to the source port will be available to be
    /// read at the destination port.
    ///
    /// # Preconditions
    ///
    /// - The port types must be identical.
    /// - The JackPortFlags of the source_port must include `PORT_IS_OUTPUT`.
    /// - The JackPortFlags of the destination_port must include `PORT_IS_INPUT`.
    ///
    /// # Errors
    ///
    /// - `InvalidPortType`: when the types are not identical
    /// - `InvalidPortFlags`: when the flags do not satisfy the preconditions above
    /// - `UnknownErrorCode`
    pub fn connect_ports(&mut self, from: &JackPort, to: &JackPort) -> JackResult<()> {
        self.connect_or_disconnect_ports(from, to, true)
    }
    /// Remove a connection between two ports.
    ///
    /// When a connection exists, data written to the source port will be available to be
    /// read at the destination port.
    ///
    /// # Preconditions
    ///
    /// - The port types must be identical.
    /// - The JackPortFlags of the source_port must include `PORT_IS_OUTPUT`.
    /// - The JackPortFlags of the destination_port must include `PORT_IS_INPUT`.
    ///
    /// # Errors
    ///
    /// - `InvalidPortType`: when the types are not identical
    /// - `InvalidPortFlags`: when the flags do not satisfy the preconditions above
    /// - `UnknownErrorCode`
    pub fn disconnect_ports(&mut self, from: &JackPort, to: &JackPort) -> JackResult<()> {
        self.connect_or_disconnect_ports(from, to, false)
    }
    /// Get a port from the JACK server by its name.
    ///
    /// # Errors
    ///
    /// - `PortNotFound`: if no port with that name was found
    /// - `NulError`: if any `&str` argument contains a NUL byte (`\0`).
    pub fn get_port_by_name(&self, name: &str) -> JackResult<JackPort> {
        let name = str_to_cstr(name)?;
        let ptr = unsafe {
            jack_port_by_name(self.handle, name.as_ptr())
        };
        if ptr.is_null() {
            Err(ErrorKind::PortNotFound)?
        }
        unsafe {
            Ok((JackPort::from_ptr(ptr)))
        }
    }
    /// Get all (or a selection of) ports available in the JACK server.
    ///
    /// # Parameters
    ///
    /// - port_filter: A regular expression used to select ports by name. If `None`, no
    /// selection based on name will be carried out.
    /// - type_filter: A regular expression used to select ports by type. If `None`, no
    /// selection based on type will be carried out.
    /// - flags_filter: A value used to select ports by their flags. If `None`, no
    /// selection based on flags will be carried out.
    ///
    /// # Errors
    ///
    /// - `NulError`: if any `&str` argument contains a NUL byte (`\0`).
    /// - `ProgrammerError`: if I've made a mistake, or your program is utterly degenerate
    pub fn get_ports(&self, port_filter: Option<&str>, type_filter: Option<&str>, flags_filter: Option<JackPortFlags>) -> JackResult<Vec<JackPort>> {
        let mut flags = JackPortFlags::empty();
        let mut pf = CString::new("").unwrap();
        let mut tf = CString::new("").unwrap();
        if let Some(f) = flags_filter {
            flags = f;
        }
        if let Some(f) = port_filter {
            pf = str_to_cstr(f)?;
        }
        if let Some(f) = type_filter {
            tf = str_to_cstr(f)?;
        }
        let mut ptr = unsafe {
            jack_get_ports(self.handle, pf.as_ptr(), tf.as_ptr(), flags.bits())
        };
        if ptr.is_null() {
            Err(ErrorKind::ProgrammerError)?
        }
        let mut cstrs: Vec<&CStr> = vec![];
        loop {
            unsafe {
                if (*ptr).is_null() {
                    break;
                }
                else {
                    let cs = CStr::from_ptr(*ptr);
                    cstrs.push(cs);
                    ptr = ptr.offset(1);
                }
            }
        }
        let mut ret: Vec<JackPort> = vec![];
        for st in cstrs {
            let ptr = unsafe {
                jack_port_by_name(self.handle, st.as_ptr())
            };
            if !ptr.is_null() {
                unsafe {
                    ret.push(JackPort::from_ptr(ptr));
                }
            }
        }
        Ok(ret)
    }
    /// Create a new port for the client.
    ///
    /// This is an object used for moving data of any type in or out of the client.
    /// Ports may be connected in various ways.
    ///
    /// Each port has a short name. The port's full name contains the name of the client
    /// concatenated with a colon (:) followed by its short name. The jack_port_name_size()
    /// is the maximum length of this full name. Exceeding that will cause the port
    /// registration to fail and return `ProgrammerError`.
    ///
    /// The port_name must be unique among all ports owned by this client. If the name is
    /// not unique, the registration will fail.
    ///
    /// All ports have a type, which may be any non-NULL and non-zero length string,
    /// passed as an argument. Some port types are built into the JACK API, like
    /// JACK_DEFAULT_AUDIO_TYPE or JACK_DEFAULT_MIDI_TYPE. *[By default, sqa-jack makes a
    /// JACK_DEFAULT_AUDIO_TYPE port - this will be changeable in later releases.]*
    ///
    /// # Errors
    ///
    /// - `NulError`: if any `&str` argument contains a NUL byte (`\0`).
    /// - `PortRegistrationFailed`: if port registration failed (TODO: why could this happen?)
    pub fn register_port(&mut self, name: &str, ty: JackPortFlags) -> JackResult<JackPort> {
        let ptr = unsafe {
            let name = str_to_cstr(name)?;
            jack_port_register(self.handle, name.as_ptr(), JACK_DEFAULT_AUDIO_TYPE.as_ptr() as *const i8, ty.bits(), 0)
        };
        if ptr.is_null() {
            Err(ErrorKind::PortRegistrationFailed)?
        }
        else {
            unsafe {
                Ok(JackPort::from_ptr(ptr))
            }
        }
    }
    /// Remove the port from the client, disconnecting any existing connections.
    ///
    /// # Errors
    ///
    /// - `PortNotMine`: if you deregister a port that this client doesn't own
    /// - `InvalidPort`
    /// - `UnknownErrorCode`
    pub fn unregister_port(&mut self, port: JackPort) -> JackResult<()> {
        let mine = unsafe {
            jack_port_is_mine(self.handle, port.as_ptr())
        };
        if mine != 0 {
            Err(ErrorKind::PortNotMine)?
        }
        let code = unsafe {
            jack_port_unregister(self.handle, port.as_ptr())
        };
        match code {
            0 => Ok(()),
            -1 => Err(ErrorKind::InvalidPort)?,
            x @ _ => Err(ErrorKind::UnknownErrorCode("unregister_port()", x))?
        }
    }
}
impl JackConnection<Deactivated> {
    /// Open an external client session with a JACK server, optionally specifying
    /// a number of `JackOpenOptions`.
    ///
    /// # Errors
    ///
    /// - `JackOpenFailed(status)`: if the connection could not be opened. Contains a
    /// `JackStatus` detailing what went wrong.
    /// - `NulError`: if any `&str` argument contains a NUL byte (`\0`).
    pub fn connect(client_name: &str, opts: Option<JackOpenOptions>) -> JackResult<Self> {
        let mut status = 0;
        let opts = opts.map(|x| x.bits()).unwrap_or(JackNullOption);
        let client = unsafe {
            let name = str_to_cstr(client_name)?;
            jack_client_open(name.as_ptr(), opts, &mut status)
        };
        if client.is_null() {
            Err(ErrorKind::JackOpenFailed(
                JackStatus::from_bits_truncate(status)
            ))?;
        }
        let sample_rate = unsafe { jack_get_sample_rate(client) };
        Ok(JackConnection {
            handle: client,
            sample_rate: sample_rate,
            _phantom: PhantomData
        })
    }
    /// Register a handler (a struct that implements `JackHandler`).
    ///
    /// # Safety
    ///
    /// **Warning:** Your handler will never be deallocated / `Drop`ped.
    ///
    /// # Errors
    ///
    /// - `UnknownErrorCode`
    pub fn set_handler<F>(&mut self, handler: F) -> JackResult<()> where F: JackHandler {
        handler::set_handler(self, handler)
    }
    /// Tell the Jack server that the program is ready to start processing audio.
    ///
    /// # Returns
    ///
    /// Returns the `Activated` connection type on success, or the current structure and
    /// an error on failure.
    ///
    /// # Errors
    ///
    /// - `UnknownErrorCode`
    pub fn activate(self) -> Result<JackConnection<Activated>, (Self, errors::Error)> {
        unsafe {
            self.activate_or_deactivate(true)
        }
    }
}
impl JackConnection<Activated> {
    /// Tell the Jack server to remove this client from the process graph.
    /// Also, **disconnect all ports belonging to it**, since inactive clients have no port
    /// connections.
    ///
    /// # Returns
    ///
    /// Returns the `Dectivated` connection type on success, or the current structure and
    /// an error on failure.
    ///
    /// # Errors
    ///
    /// - `UnknownErrorCode`
    pub fn deactivate(self) -> Result<JackConnection<Deactivated>, (Self, errors::Error)> {
        unsafe {
            self.activate_or_deactivate(false)
        }
    }
}
impl<T> Drop for JackConnection<T> {
    fn drop(&mut self) {
        unsafe {
            jack_deactivate(self.handle);
            jack_client_close(self.handle);
        }
    }
}
