use state::{Context, ThreadNotifier, Message, ChainType, CommandState};
use std::sync::mpsc::{Sender};
use portaudio as pa;
use mio;
use mio::{Handler, EventLoop};
use cues::QRunner;

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
                self.attach_chn(Some(ChainType::Unattached), uu);

                if let CommandState::Ready = self.desc_cmd(uu).state {
                    let mut cmd = self.commands.get_mut(&uu).unwrap().box_clone();
                    cmd.load(self, evl, uu);
                    self.commands.insert(uu, cmd);
                }
            },
            Message::SetHunk(uu, idx, val) => {
                {
                    let mut cmd = self.commands.get_mut(&uu).unwrap();
                    let ref mut hunk = cmd.get_hunks()[idx];
                    hunk.set_val(::std::ops::DerefMut::deref_mut(cmd), val);
                }
                if let CommandState::Ready = self.desc_cmd(uu).state {
                    let mut cmd = self.commands.get_mut(&uu).unwrap().box_clone();
                    cmd.load(self, evl, uu);
                    self.commands.insert(uu, cmd);
                }
                update = Some(uu);
            },
            Message::Execute(uu) => {
                let mut cmd = self.commands.get_mut(&uu).unwrap().box_clone();
                let finished = cmd.execute(self, evl, uu).unwrap();
                self.commands.insert(uu, cmd);
                update = Some(uu);

                if finished {
                    println!("calling EC on exec");
                    self.execution_completed(uu);
                }
            },
            Message::Update(uu, cu) => {
                if {
                    let mut cmd = self.commands.get_mut(&uu).unwrap();
                    cu(::std::ops::DerefMut::deref_mut(cmd))
                } {
                    println!("calling EC after update");
                    self.execution_completed(uu);
                }
                update = Some(uu);
            },
            Message::Delete(uu) => {
                self.attach_chn(None, uu);
                self.label(None, uu);
                self.commands.remove(&uu);
                self.send(Message::Deleted(uu));
            },
            Message::Attach(uu, ct) => {
                self.attach_chn(Some(ct), uu);
            },
            Message::Go(ct) => {
                let qr = QRunner::new(self.chains.get(&ct).unwrap().clone(), self, evl);
                self.runners.push(qr);
            },
            Message::QRunnerBlocked(uu, blk) => {
                for qrx in self.runners.iter_mut() {
                    if qrx.uuid() == uu {
                        qrx.blocked = Some(blk);
                    }
                }
            },
            Message::QRunnerCompleted(uu) => {
                self.runners.retain(|uc| uc.uuid() != uu);
            },
            Message::ExecutionCompleted(uu) => { self.execution_completed(uu); },
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
