use *;

#[derive(Copy, Clone, Debug)]
pub struct JackPort {
    ptr: *mut jack_port_t,
}
unsafe impl Send for JackPort {}

impl JackPort {
    pub fn as_ptr(&self) -> *mut jack_port_t {
        self.ptr
    }
    pub unsafe fn from_ptr(ptr: *mut jack_port_t) -> Self {
        JackPort {
            ptr: ptr
        }
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
    pub fn get_flags(&self) -> JackPortFlags {
        let flags = unsafe { jack_port_flags(self.ptr) };
        JackPortFlags::from_bits_truncate(flags as u64)
    }
}
