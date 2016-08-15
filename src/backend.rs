use state::{Context, ThreadNotifier, Message};
use std::sync::mpsc::{Sender};
use portaudio as pa;
use mio;
use mio::{Handler, EventLoop};
use std::rc::Rc;
use std::cell::RefCell;

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
                let mut cmd = self.commands.get_mut(&uu).unwrap().box_clone();
                cmd.execute(self, evl, uu).unwrap();
                self.commands.insert(uu, cmd);
                update = Some(uu);
            },
            Message::Update(uu, cu) => {
                let mut cmd = self.commands.get_mut(&uu).unwrap();
                cu(::std::ops::DerefMut::deref_mut(cmd));
                update = Some(uu);
            },
            Message::Delete(uu) => {
                self.commands.remove(&uu);
                self.send(Message::Deleted(uu));
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
    let mut ctx = Context::new(&mut p, tx, tn);
    let mut evl: EventLoop<Context> = EventLoop::new().unwrap();
    println!("sending..");
    stx.send(evl.channel()).unwrap();
    println!("done");
    evl.run(&mut ctx).unwrap();
}
