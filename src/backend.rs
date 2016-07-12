use std::sync::{Arc, Mutex};
use state::{ReadableContext, WritableContext, ThreadNotifier, ActionDescriptor, ActionState};
use std::sync::mpsc::{Sender};
use mixer;
use command::Command;
use portaudio as pa;
use mio;
use mio::{Handler, EventLoop};
use uuid::Uuid;
use chrono::Duration;

pub type BackendSender = mio::Sender<BackendMessage>;
pub trait BackendTimeout {
    fn execute(&mut self, ctx: &mut WritableContext, evl: &mut EventLoop<WritableContext>) -> Option<u64>;
}

pub enum BackendMessage {
    Execute(Box<Command>),
    DescChange(Uuid, String),
    StateChange(Uuid, ActionState),
    RuntimeChange(Uuid, Duration)
}
impl<'a> Handler for WritableContext<'a> {
    type Timeout = Box<BackendTimeout>;
    type Message = BackendMessage;

    fn timeout(&mut self, evl: &mut EventLoop<Self>, mut timeout: Box<BackendTimeout>) {
        if let Some(next_int) = timeout.execute(self, evl) {
            evl.timeout_ms(timeout, next_int).unwrap();
        }
    }
    fn notify(&mut self, evl: &mut EventLoop<Self>, msg: Self::Message) {
        match msg {
            BackendMessage::Execute(mut cmd) => {
                let mut ad = ActionDescriptor::new(cmd.name().to_owned());
                let mut complete: bool = false;
                ad.state = ActionState::Running;
                let dur = Duration::span(|| {
                    complete = cmd.execute(self, evl, ad.uuid).unwrap();
                });
                if complete {
                    ad.state = ActionState::Completed;
                    ad.runtime = dur;
                }
                println!("new cmd: {:?}", ad);
                self.acts.push(ad);
            },
            BackendMessage::DescChange(uu, newdesc) => {
                if let Some(ad) = self.get_action_desc_mut(uu) {
                    ad.desc = newdesc;
                }
                else { panic!("notify() called with invalid UUID for change request") }
            },
            BackendMessage::StateChange(uu, newstate) => {
                if let Some(ad) = self.get_action_desc_mut(uu) {
                    ad.state = newstate;
                    println!("state change for cmd: {:?}", ad);
                }
                else { panic!("notify() called with invalid UUID for change request") }
            },
            BackendMessage::RuntimeChange(uu, rnt) => {
                if let Some(ad) = self.get_action_desc_mut(uu) {
                    ad.runtime = rnt;
                    println!("rt change for cmd: {:?}", ad);
                }
                else { panic!("notify() called with invalid UUID for change request") }
            }
        }
        self.update();
    }
}
pub fn backend_main(rc: Arc<Mutex<ReadableContext>>, stx: Sender<BackendSender>, tn: ThreadNotifier) {
    /* THE PORTAUDIO CONTEXT MUST BE THE FIRST BOUND VARIABLE
     * HEED THIS WARNING, OR THE BORROW CHECKER WILL SMITE THEE */
    let mut p = pa::PortAudio::new().unwrap();
    let mut ctx = WritableContext::new(rc, tn);
    let idx = p.default_output_device().unwrap();
    ctx.insert_device(mixer::DeviceSink::from_device_chans(&mut p, idx).unwrap());
    let mut evl: EventLoop<WritableContext> = EventLoop::new().unwrap();
    println!("sending..");
    stx.send(evl.channel()).unwrap();
    println!("done");
    evl.run(&mut ctx).unwrap();
}
