mod bindgen;
use bindgen::{SF_INFO, SNDFILE, sf_count_t, sf_open, sf_readf_float, sf_close, sf_error, sf_error_number, sf_strerror, FileFormat, OpenMode, SfError};

extern crate libc;
extern crate num;
#[macro_use] extern crate enum_primitive;
use std::ffi::{CString, NulError, CStr};
use libc::{c_char, c_int, int64_t};
use num::FromPrimitive;
#[derive(Debug)]
struct SndFileInfo {
    frames: sf_count_t,
    samplerate: c_int,
    channels: c_int,
    format: FileFormat,
    sections: c_int,
    seekable: c_int
}
impl From<SF_INFO> for SndFileInfo {
    fn from(info: SF_INFO) -> Self {
        // XXX: why do we need to take 2 from info.format?
        SndFileInfo {
            frames: info.frames,
            samplerate: info.samplerate,
            channels: info.channels,
            format: FileFormat::from_i32(info.format - 2).expect("Invalid FileFormat provided"),
            sections: info.sections,
            seekable: info.seekable
        }
    }
}
#[derive(Debug)]
struct SndFile {
    pub ptr: *mut SNDFILE,
    pub info: SndFileInfo
}
impl Drop for SndFile {
    fn drop(&mut self) {
        unsafe { sf_close(self.ptr) };
    }
}
#[derive(Debug)]
struct SndError {
    err: SfError,
    pub expl: String
}
fn get_error(sf: *mut SNDFILE) -> SndError {
    let err: SfError;
    let strptr: &CStr;
    let mut errno: c_int;
    unsafe {
        errno = sf_error(sf);
        if errno >= SfError::SF_ERR_UNLISTED as i32 {
            strptr = CStr::from_ptr(sf_error_number(errno));
            errno = SfError::SF_ERR_UNLISTED as i32;
        }
        else {
            strptr = CStr::from_ptr(sf_strerror(sf));
        }
    }
    err = SfError::from_i32(errno).unwrap();
    let expl = strptr.to_string_lossy().into_owned();
    SndError {
        err: err,
        expl: expl
    }
}
#[derive(Debug)]
enum BadStuff {
    NulError(NulError),
    SndError(SndError)
}
fn open(path: &str, mode: OpenMode) -> Result<SndFile, BadStuff> {
    let cstr = try!(CString::new(path).map_err(|e| BadStuff::NulError(e)));
    let mut sfi = SF_INFO {
        frames: 0,
        samplerate: 0,
        channels: 0,
        format: 0,
        sections: 0,
        seekable: 0
    };
    let sndfile_ptr: *mut SNDFILE;
    unsafe {
        sndfile_ptr = sf_open(cstr.as_ptr(), mode as c_int, &mut sfi);
    }
    if sndfile_ptr.is_null() {
        Err(BadStuff::SndError(get_error(sndfile_ptr)))
    }
    else {
        Ok(SndFile {
            ptr: sndfile_ptr,
            info: SndFileInfo::from(sfi)
        })
    }
}

fn main() {
    let sfi = open("test.aiff", OpenMode::SFM_READ);
    println!("{:?}", sfi);
    println!("Go go gadget PortAudio!");
    println!("{:?}", run(sfi.unwrap()));
}

extern crate portaudio;

use portaudio as pa;


const INTERLEAVED: bool = true;


fn run(file: SndFile) -> Result<(), pa::Error> {

    let pa = try!(pa::PortAudio::new());

    println!("PortAudio:");
    println!("version: {}", pa.version());
    println!("version text: {:?}", pa.version_text());
    println!("host count: {}", try!(pa.host_api_count()));

    let default_host = try!(pa.default_host_api());
    println!("default host: {:#?}", pa.host_api_info(default_host));

    let def_output = try!(pa.default_output_device());
    let output_info = try!(pa.device_info(def_output));
    println!("Default output device info: {:#?}", &output_info);

    // Construct the output stream parameters.
    let latency = output_info.default_low_output_latency;
    let output_params: pa::StreamParameters<f32> = pa::StreamParameters::new(def_output, file.info.channels, INTERLEAVED, latency);

    // Check that the stream format is supported.
    try!(pa.is_output_format_supported(output_params, file.info.samplerate as f64));

    // Construct the settings with which we'll open our duplex stream.
    let settings = pa::stream::OutputSettings::new(output_params, file.info.samplerate as f64, file.info.frames as u32);

    let mut frames_written: u64 = 0;
    // A callback to pass to the non-blocking stream.
    let callback = move |pa::stream::OutputCallbackArgs { buffer, frames, time, .. }| {
        let written: sf_count_t;
        unsafe {
            written = sf_readf_float(file.ptr, buffer.get_unchecked_mut(0), frames as i64);
        }
        println!("Wrote {}", written);
        frames_written += written as u64;
        if file.info.frames as u64 > frames_written { pa::Continue } else { pa::Complete }
    };

    // Construct a stream with input and output sample types of f32.
    let mut stream = try!(pa.open_non_blocking_stream(settings, callback));

    try!(stream.start());

    // Loop while the non-blocking stream is active.
    while let true = try!(stream.is_active()) {
    }

    try!(stream.stop());

    Ok(())
}
