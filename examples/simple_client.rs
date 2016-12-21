extern crate sqa_jack;

use sqa_jack::{JackPort, JackConnection, JackCallbackContext, JackResult, JackPortType};
use std::io::Write;
use std::thread;
struct Ports {
    inp: JackPort,
    out: JackPort
}
fn process(mut ctx: JackCallbackContext) -> i32 {
    if let Some(ports) = ctx.unstash_data::<Ports>() {
        let inp = ctx.get_port_buffer(&ports.inp).unwrap();
        let out = ctx.get_port_buffer(&ports.out).unwrap();
        unsafe {
            ::std::ptr::copy_nonoverlapping(inp as *mut [f32] as *mut f32, out as *mut [f32] as *mut f32, out.len());
        }
    }
    0
}
fn run() -> JackResult<()> {
    let mut conn = JackConnection::connect("simple_client")?;
    let ports = Box::new(Ports {
        inp: conn.register_port("input", JackPortType::Input)?,
        out: conn.register_port("output", JackPortType::Output)?
    });
    conn.stash_data(ports);
    conn.set_process_callback(process)?;
    conn.activate()?;
    thread::sleep(::std::time::Duration::new(60 * 60, 0));
    Ok(())
}
fn main() {
    println!("{:?}", run());
}
