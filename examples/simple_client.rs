extern crate sqa_jack;

use sqa_jack::{JackPort, JackConnection, JackHandler, JackControl, JackCallbackContext, JackResult, PORT_IS_INPUT, PORT_IS_OUTPUT, PORT_IS_PHYSICAL};
use std::thread;
struct Ports {
    inp: JackPort,
    out: JackPort
}
impl JackHandler for Ports {
    fn process(&mut self, ctx: &JackCallbackContext) -> JackControl {
        let inp = ctx.get_port_buffer(&self.inp).unwrap();
        let out = ctx.get_port_buffer(&self.out).unwrap();
        for (out, inp) in out.iter_mut().zip(inp.iter()) {
            *out = *inp;
        }
        JackControl::Continue
    }
}
fn run() -> JackResult<()> {
    let mut conn = JackConnection::connect("simple_client")?;
    let inp = conn.register_port("input", PORT_IS_INPUT)?;
    let out = conn.register_port("output", PORT_IS_OUTPUT)?;
    let ports = Ports {
        inp: inp,
        out: out
    };
    conn.set_handler(ports)?;
    let mut conn = match conn.activate() {
        Ok(nc) => nc,
        Err((_, err)) => return Err(err)
    };
    let ports = conn.get_ports(None, None, Some(PORT_IS_INPUT | PORT_IS_PHYSICAL))?;
    if ports.len() >= 1 {
        conn.connect_ports(&out, &ports[0])?;
        println!("Connected output port to {}", ports[0].get_name(false)?);
    }
    let ports = conn.get_ports(None, None, Some(PORT_IS_OUTPUT | PORT_IS_PHYSICAL))?;
    if ports.len() >= 1 {
        conn.connect_ports(&ports[0], &inp)?;
        println!("Connected input port to {}", ports[0].get_name(false)?);
    }
    thread::sleep(::std::time::Duration::new(60 * 60, 0));
    Ok(())
}
fn main() {
    println!("{:?}", run());
}
