use std::sync::{Arc, Mutex};
use state::{ReadableContext, WritableContext, ObjectType};
use std::sync::mpsc::Receiver;
use mixer;
use command::Command;
use portaudio as pa;
pub fn backend_main(rc: Arc<Mutex<ReadableContext>>, rx: Receiver<Box<Command>>) {
    let mut p = pa::PortAudio::new().unwrap();
    let mut ctx = WritableContext::new(rc);
    let idx = p.default_output_device().unwrap();
    ctx.insert_device(mixer::DeviceSink::from_device_chans(&mut p, idx).unwrap());
    while let Ok(mut cmd) = rx.recv() {
        cmd.execute(&mut ctx);
        ctx.update();
    }
}
