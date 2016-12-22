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
use std::ptr;
use std::borrow::Cow;
use errors::{ErrorKind, ChainErr};
pub use errors::JackResult;
pub use handler::{JackCallbackContext, JackControl, JackHandler};
pub use port::JackPort;
bitflags! {
    pub flags JackStatus: libc::c_uint {
        const STATUS_FAILURE = jack_sys::JackFailure,
        const STATUS_INVALID_OPTION = jack_sys::JackInvalidOption,
        const STATUS_NAME_NOT_UNIQUE = jack_sys::JackNameNotUnique,
        const STATUS_SERVER_STARTED = jack_sys::JackServerStarted,
        const STATUS_SERVER_FAILED = jack_sys::JackServerFailed,
        const STATUS_SERVER_ERROR = jack_sys::JackServerError,
        const STATUS_NO_SUCH_CLIENT = jack_sys::JackNoSuchClient,
        const STATUS_LOAD_FAILURE = jack_sys::JackLoadFailure,
        const STATUS_INIT_FAILURE = jack_sys::JackInitFailure,
        const STATUS_SHM_FAILURE = jack_sys::JackShmFailure,
        const STATUS_VERSION_ERROR = jack_sys::JackShmFailure
    }
}
bitflags! {
    pub flags JackPortFlags: libc::c_ulong {
        const PORT_IS_INPUT = JackPortIsInput as libc::c_ulong,
        const PORT_IS_OUTPUT = JackPortIsOutput as libc::c_ulong,
        const PORT_IS_PHYSICAL = JackPortIsPhysical as libc::c_ulong,
        const PORT_CAN_MONITOR = JackPortCanMonitor as libc::c_ulong,
        const PORT_IS_TERMINAL = JackPortIsTerminal as libc::c_ulong
    }
}
pub struct Deactivated;
pub struct Activated;
pub struct JackConnection<T> {
    handle: *mut jack_client_t,
    sample_rate: u32,
    _phantom: PhantomData<T>
}
fn str_to_cstr(st: &str) -> JackResult<CString> {
    Ok(CString::new(st).chain_err(|| ErrorKind::NulError)?)
}

impl<T> JackConnection<T> {
    pub fn as_ptr(&self) -> *const jack_client_t {
        self.handle
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
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
    pub fn connect_ports(&mut self, from: &JackPort, to: &JackPort) -> JackResult<()> {
        self.connect_or_disconnect_ports(from, to, true)
    }
    pub fn disconnect_ports(&mut self, from: &JackPort, to: &JackPort) -> JackResult<()> {
        self.connect_or_disconnect_ports(from, to, false)
    }
    pub fn get_ports(&self, flags_filter: Option<JackPortFlags>) -> JackResult<Vec<JackPort>> {
        let mut flags = JackPortFlags::empty();
        if let Some(f) = flags_filter {
            flags = f;
        }
        let mut ptr = unsafe {
            jack_get_ports(self.handle, ptr::null(), ptr::null(), flags.bits())
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
}
impl JackConnection<Deactivated> {
    pub fn connect(client_name: &str) -> JackResult<Self> {
        let mut status = 0;
        let client = unsafe {
            let name = str_to_cstr(client_name)?;
            jack_client_open(name.as_ptr(), JackNullOption, &mut status)
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
    pub fn register_port(&mut self, name: &str, ty: JackPortFlags) -> JackResult<JackPort> {
        let ptr = unsafe {
            let name = str_to_cstr(name)?;
            jack_port_register(self.handle, name.as_ptr(), JACK_DEFAULT_AUDIO_TYPE.as_ptr() as *const i8, ty.bits(), 0)
        };
        if ptr.is_null() {
            Err(ErrorKind::ProgrammerError)?
        }
        else {
            unsafe {
                Ok(JackPort::from_ptr(ptr))
            }
        }
    }
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
    pub fn set_handler<F>(&mut self, handler: F) -> JackResult<()> where F: JackHandler {
        handler::set_handler(self, handler)
    }
    pub fn activate(self) -> Result<JackConnection<Activated>, (Self, errors::Error)> {
        unsafe {
            self.activate_or_deactivate(true)
        }
    }
}
impl JackConnection<Activated> {
    pub fn deactivate(self) -> Result<JackConnection<Deactivated>, (Self, errors::Error)> {
        unsafe {
            self.activate_or_deactivate(false)
        }
    }
}
impl<T> Drop for JackConnection<T> {
    fn drop(&mut self) {
        unsafe {
            jack_client_close(self.handle);
        }
    }
}
