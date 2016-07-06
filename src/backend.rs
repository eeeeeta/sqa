use std::sync::{Arc, Mutex};
use state::{ReadableContext, WritableContext};
use std::sync::mpsc::{Sender};
use mixer;
use command::Command;
use portaudio as pa;
use mio;
use mio::{Handler, EventLoop};

pub trait BackendTimeout {
    fn execute(&mut self, ctx: &mut WritableContext, evl: &mut EventLoop<WritableContext>) -> Option<u64>;
}

impl<'a> Handler for WritableContext<'a> {
    type Timeout = Box<BackendTimeout>;
    type Message = Box<Command>;

    fn timeout(&mut self, evl: &mut EventLoop<Self>, mut timeout: Box<BackendTimeout>) {
        if let Some(next_int) = timeout.execute(self, evl) {
            evl.timeout_ms(timeout, next_int).unwrap();
        }
        self.update();
    }
    fn notify(&mut self, evl: &mut EventLoop<Self>, mut msg: Self::Message) {
        msg.execute(self, evl).unwrap();
        self.update();
    }
}
pub fn backend_main(rc: Arc<Mutex<ReadableContext>>, stx: Sender<mio::Sender<Box<Command>>>) {
    /* THE PORTAUDIO CONTEXT MUST BE THE FIRST BOUND VARIABLE
     * HEED THIS WARNING, OR THE BORROW CHECKER WILL SMITE THEE */
    let mut p = pa::PortAudio::new().unwrap();
    let mut ctx = WritableContext::new(rc);
    let idx = p.default_output_device().unwrap();
    ctx.insert_device(mixer::DeviceSink::from_device_chans(&mut p, idx).unwrap());
    let mut evl: EventLoop<WritableContext> = EventLoop::new().unwrap();
    stx.send(evl.channel()).unwrap();
    evl.run(&mut ctx).unwrap();
}
