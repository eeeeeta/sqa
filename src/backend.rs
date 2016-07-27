use std::sync::{Arc, Mutex};
use state::{Context, ThreadNotifier, Message};
use std::sync::mpsc::{Sender};
use mixer;
use command::Command;
use portaudio as pa;
use mio;
use mio::{Handler, EventLoop};
use uuid::Uuid;
use chrono::Duration;

pub type BackendSender = mio::Sender<Message>;
pub trait BackendTimeout {
    fn execute(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>) -> Option<u64>;
}

impl<'a> Handler for Context<'a> {
    type Timeout = Box<BackendTimeout>;
    type Message = Message;

    fn timeout(&mut self, evl: &mut EventLoop<Self>, mut timeout: Box<BackendTimeout>) {
        if let Some(next_int) = timeout.execute(self, evl) {
            evl.timeout_ms(timeout, next_int).unwrap();
        }
    }
    fn notify(&mut self, evl: &mut EventLoop<Self>, msg: Self::Message) {
        let mut update = None;
        match msg {
            Message::NewCmd(uu, spawner) => {
                assert!(self.commands.insert(uu, spawner.spawn()).is_none());
                update = Some(uu);
            },
            Message::SetHunk(uu, idx, val) => {
                let mut cmd = self.commands.get_mut(&uu).unwrap();
                let ref mut hunk = cmd.get_hunks()[idx];
                hunk.set_val(::std::ops::DerefMut::deref_mut(cmd), val);
                update = Some(uu);
            },
            Message::Execute(uu) => {
                // FIXME: cloning & borrowing mess
                let mut cmd = self.commands.get_mut(&uu).unwrap().box_clone();
                cmd.execute(self, evl, uu).unwrap();
                update = Some(uu);
            },
            _ => unimplemented!()
        }
        if let Some(uu) = update {
            self.update_cmd(uu);
        }
    }
}
pub fn backend_main(stx: Sender<BackendSender>, tx: Sender<Message>, tn: ThreadNotifier) {
    /* THE PORTAUDIO CONTEXT MUST BE THE FIRST BOUND VARIABLE
     * HEED THIS WARNING, OR THE BORROW CHECKER WILL SMITE THEE */
    let mut p = pa::PortAudio::new().unwrap();
    let mut ctx = Context::new(tx, tn);
    let idx = p.default_output_device().unwrap();
    ctx.insert_device(mixer::DeviceSink::from_device_chans(&mut p, idx).unwrap());
    let mut evl: EventLoop<Context> = EventLoop::new().unwrap();
    println!("sending..");
    stx.send(evl.channel()).unwrap();
    println!("done");
    evl.run(&mut ctx).unwrap();
}
