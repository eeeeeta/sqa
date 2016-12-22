#[macro_use]
extern crate bitflags;
extern crate libc;
extern crate jack_sys;
#[macro_use]
extern crate error_chain;

static JACK_DEFAULT_AUDIO_TYPE: &'static [u8] = b"32 bit float mono audio\0";
pub mod errors;

use jack_sys::*;
use std::ffi::{CString, CStr};
use std::any::Any;
use std::marker::PhantomData;
use std::ptr;
use std::borrow::Cow;
use errors::{ErrorKind, ChainErr};
pub use errors::JackResult;

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
    callbacks: Box<Callbacks>,
    _phantom: PhantomData<T>
}
pub struct JackCallbackContext {
    userdata: *mut Option<Box<Any>>,
    nframes: jack_nframes_t
}
#[derive(Copy, Clone, Debug)]
pub struct JackPort {
    ptr: *mut jack_port_t,
}
struct Callbacks {
    process: Option<Box<FnMut(JackCallbackContext) -> i32>>,
    userdata: Option<Box<Any>>
}

fn str_to_cstr(st: &str) -> JackResult<CString> {
    Ok(CString::new(st).chain_err(|| ErrorKind::NulError)?)
}
extern "C" fn process_callback(nframes: jack_nframes_t, user: *mut libc::c_void) -> i32 {
    unsafe {
        let callbacks = &mut *(user as *mut Box<Callbacks>);
        let ctx = JackCallbackContext {
            userdata: &mut callbacks.userdata,
            nframes: nframes
        };
        callbacks.process.as_mut().map(|f| f(ctx)).unwrap_or(-1)
    }
}

impl JackCallbackContext {
    #[inline(always)]
    pub fn nframes(&self) -> u32 {
        self.nframes
    }
    pub fn unstash_data<T>(&mut self) -> Option<&'static mut T> where T: 'static {
        let userdata = unsafe { &mut (*self.userdata) };
        if let Some(ref mut data) = *userdata {
            if let Some(t) = data.downcast_mut::<T>() {
                Some(t)
            }
            else {
                None
            }
        }
        else {
            None
        }
    }
    pub fn get_port_buffer(&self, port: &JackPort) -> Option<&mut [f32]> {
        unsafe {
            let buf = jack_port_get_buffer(port.ptr, self.nframes);
            if buf.is_null() {
                None
            }
            else {
                Some(::std::slice::from_raw_parts_mut(buf as *mut f32, self.nframes as usize))
            }
        }
    }
}
impl JackPort {
    pub fn as_ptr(&self) -> *const jack_port_t {
        self.ptr
    }
    pub fn set_short_name(&mut self, name: &str) -> JackResult<()> {
        let code = unsafe {
            let name = str_to_cstr(name)?;
            jack_port_set_name(self.ptr, name.as_ptr())
        };
        if code != 0 {
            Err(ErrorKind::ProgrammerError)?
        }
        else {
            Ok(())
        }
    }
    pub fn get_name(&self, short: bool) -> JackResult<Cow<str>> {
        unsafe {
            let ptr = self.get_name_raw(short)?;
            Ok(CStr::from_ptr(ptr).to_string_lossy())
        }
    }
    pub fn get_type(&self) -> JackResult<Cow<str>> {
        unsafe {
            let ptr = jack_port_type(self.ptr);
            if ptr.is_null() {
                Err(ErrorKind::InvalidPort)?
            }
            Ok(CStr::from_ptr(ptr).to_string_lossy())
        }
    }
    unsafe fn get_name_raw(&self, short: bool) -> JackResult<*const libc::c_char> {
        let ptr = if short {
            jack_port_short_name(self.ptr)
        }
        else {
            jack_port_name(self.ptr)
        };
        if ptr.is_null() {
            Err(ErrorKind::InvalidPort)?
        }
        else {
            Ok(ptr)
        }
    }
    pub fn get_flags(&self) -> JackPortFlags {
        let flags = unsafe { jack_port_flags(self.ptr) };
        JackPortFlags::from_bits_truncate(flags as u64)
    }
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
                ret.push(JackPort { ptr: ptr });
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
            callbacks: Box::new(Callbacks {
                process: None,
                userdata: None
            }),
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
            Ok(JackPort { ptr: ptr })
        }
    }
    pub fn unregister_port(&mut self, port: JackPort) -> JackResult<()> {
        let code = unsafe {
            jack_port_unregister(self.handle, port.ptr)
        };
        match code {
            0 => Ok(()),
            -1 => Err(ErrorKind::InvalidPort)?,
            x @ _ => Err(ErrorKind::UnknownErrorCode("unregister_port()", x))?
        }
    }
    pub fn stash_data(&mut self, data: Box<Any>) {
        self.callbacks.userdata = Some(data);
    }
    pub fn set_process_callback<F>(&mut self, cb: F) -> JackResult<()> where F: FnMut(JackCallbackContext) -> i32 + 'static {
        self.callbacks.process = Some(Box::new(cb));
        let user_ptr = &mut self.callbacks as *mut Box<Callbacks> as *mut libc::c_void;
        let code = unsafe {
            jack_set_process_callback(self.handle, Some(process_callback), user_ptr)
        };
        if code != 0 {
            Err(ErrorKind::UnknownErrorCode("set_process_callback()", code))?
        }
        else {
            Ok(())
        }
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
