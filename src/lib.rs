#![allow(non_upper_case_globals)]

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
use std::borrow::Cow;
use errors::{ErrorKind, ChainErr};
pub use errors::JackResult;

bitflags! {
    pub flags JackStatus: libc::c_uint {
        const JackFailure = jack_sys::JackFailure,
        const JackInvalidOption = jack_sys::JackInvalidOption,
        const JackNameNotUnique = jack_sys::JackNameNotUnique,
        const JackServerStarted = jack_sys::JackServerStarted,
        const JackServerFailed = jack_sys::JackServerFailed,
        const JackServerError = jack_sys::JackServerError,
        const JackNoSuchClient = jack_sys::JackNoSuchClient,
        const JackLoadFailure = jack_sys::JackLoadFailure,
        const JackInitFailure = jack_sys::JackInitFailure,
        const JackShmFailure = jack_sys::JackShmFailure,
        const JackVersionError = jack_sys::JackShmFailure
    }
}
pub struct JackConnection {
    handle: *mut jack_client_t,
    sample_rate: u32,
    activated: bool,
    callbacks: Box<Callbacks>
}
pub struct JackCallbackContext {
    userdata: *mut Option<Box<Any>>,
    nframes: jack_nframes_t
}
pub struct JackPort {
    ptr: *mut jack_port_t,
    ty: JackPortType
}
#[derive(Copy, Clone)]
pub enum JackPortType {
    Input,
    Output,
}
impl JackPortType {
    fn to_flags(&self) -> libc::c_ulong {
        use JackPortType::*;
        match *self {
            Input => JackPortIsInput as libc::c_ulong,
            Output => JackPortIsOutput as libc::c_ulong,
        }
    }
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
    pub fn set_name(&mut self, name: &str) -> JackResult<()> {
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
    pub fn get_name(&self) -> JackResult<Cow<str>> {
        unsafe {
            let ptr = jack_port_short_name(self.ptr);
            if ptr.is_null() {
                Err(ErrorKind::InvalidPort)?
            }
            else {
                Ok(CStr::from_ptr(ptr).to_string_lossy())
            }
        }
    }
    pub fn ty(&self) -> JackPortType {
        self.ty
    }
}
impl JackConnection {
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
            activated: false,
            callbacks: Box::new(Callbacks {
                process: None,
                userdata: None
            })
        })
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn register_port(&mut self, name: &str, ty: JackPortType) -> JackResult<JackPort> {
        let ptr = unsafe {
            let name = str_to_cstr(name)?;
            jack_port_register(self.handle, name.as_ptr(), JACK_DEFAULT_AUDIO_TYPE.as_ptr() as *const i8, ty.to_flags(), 0)
        };
        if ptr.is_null() {
            Err(ErrorKind::ProgrammerError)?
        }
        else {
            Ok(JackPort { ptr: ptr, ty: ty })
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
    pub fn stash_data(&mut self, data: Box<Any>) -> Option<Box<Any>> {
        if self.activated {
            Some(data)
        }
        else {
            self.callbacks.userdata = Some(data);
            None
        }
    }
    pub fn set_process_callback<F>(&mut self, cb: F) -> JackResult<()> where F: FnMut(JackCallbackContext) -> i32 + 'static {
        if self.activated {
            Err(ErrorKind::Activated)?
        }
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
    fn activate_or_deactivate(&mut self, activate: bool) -> JackResult<()> {
        let code = unsafe {
            if activate {
                jack_activate(self.handle)
            }
            else {
                jack_deactivate(self.handle)
            }
        };
        if code != 0 {
            Err(ErrorKind::UnknownErrorCode("activate()", code))?
        }
        else {
            self.activated = activate;
            Ok(())
        }
    }
    pub fn activate(&mut self) -> JackResult<()> {
        self.activate_or_deactivate(true)
    }
    pub fn deactivate(&mut self) -> JackResult<()> {
        self.activate_or_deactivate(false)
    }
}
impl Drop for JackConnection {
    fn drop(&mut self) {
        unsafe {
            jack_client_close(self.handle);
        }
    }
}
