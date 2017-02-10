use libc;
use errors::{ErrorKind, JackResult};
use super::{JackPortFlags, JackPortPtr, str_to_cstr};
use std::borrow::Cow;
use std::ffi::CStr;
use jack_sys::{jack_port_set_name, jack_port_type, jack_port_flags,
jack_port_short_name, jack_port_name};
/// An object used for moving data of any type in or out of the client.
///
/// Ports may be connected in various ways.
///
/// Each port has a short name. The port's full name contains the name of the client
/// concatenated with a colon (:) followed by its short name. The jack_port_name_size()
/// is the maximum length of this full name. Exceeding that will cause port
/// registration to fail and return `ProgrammerError`.
#[derive(Copy, Clone, Debug)]
pub struct JackPort {
    ptr: JackPortPtr,
}
unsafe impl Send for JackPort {}

impl JackPort {
    pub fn as_ptr(&self) -> JackPortPtr {
        self.ptr
    }
    pub unsafe fn from_ptr(ptr: JackPortPtr) -> Self {
        JackPort {
            ptr: ptr
        }
    }
    /// Modify a port's short name. May be called at any time.
    ///
    /// If the resulting full name
    /// (including the "client_name:" prefix) is longer than jack_port_name_size(), it will
    /// be truncated.
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
    /// Get the name of a port (short or long, determined by the `short` argument).
    pub fn get_name(&self, short: bool) -> JackResult<Cow<str>> {
        unsafe {
            let ptr = self.get_name_raw(short)?;
            Ok(CStr::from_ptr(ptr).to_string_lossy())
        }
    }
    /// Get the type string of a port.
    pub fn get_type(&self) -> JackResult<Cow<str>> {
        unsafe {
            let ptr = jack_port_type(self.ptr);
            if ptr.is_null() {
                Err(ErrorKind::InvalidPort)?
            }
            Ok(CStr::from_ptr(ptr).to_string_lossy())
        }
    }
    /// Get the raw pointer to the name of a port.
    ///
    /// # Safety
    ///
    /// This function is **not** intended for external consumption.
    pub unsafe fn get_name_raw(&self, short: bool) -> JackResult<*const libc::c_char> {
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
    /// Get the JackPortFlags of the port.
    pub fn get_flags(&self) -> JackPortFlags {
        let flags = unsafe { jack_port_flags(self.ptr) };
        JackPortFlags::from_bits_truncate(flags as u64)
    }
}
