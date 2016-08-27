use state::{Context, ThreadNotifier, Message, ChainType, CommandState};
use std::sync::mpsc::{Sender};
use portaudio as pa;
use mio;
use mio::{Handler, EventLoop};

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
                self.update_cmd(uu);
                self.attach_chn(Some(ChainType::Unattached), uu);

                if let CommandState::Ready = self.desc_cmd(uu).state {
                    self.load_cmd(uu, evl);
                }
            },
            Message::SetHunk(uu, idx, val) => {
                {
                    let mut cmd = self.commands.get_mut(&uu).unwrap();
                    let ref mut hunk = cmd.get_hunks()[idx];
                    hunk.set_val(::std::ops::DerefMut::deref_mut(cmd), val);
                }
                if let CommandState::Ready = self.desc_cmd(uu).state {
                    self.load_cmd(uu, evl);
                }
                update = Some(uu);
            },
            Message::Execute(uu) => {
                self.exec_cmd(uu, evl);
            },
            Message::Update(uu, cu) => {
                if {
                    let mut cmd = self.commands.get_mut(&uu).unwrap();
                    cu(::std::ops::DerefMut::deref_mut(cmd))
                } {
                    self.execution_completed(uu, evl);
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
            Message::Standby(ct) => {
                for (k, mut chn) in self.chains.clone().into_iter() {
                    if ct.is_some() && ct.clone().unwrap() == k {
                        chn.standby(self, evl);
                    }
                    else {
                        chn.unstandby(self, evl);
                    }
                    self.chains.insert(k, chn);
                }
            },
            Message::SetFallthru(uu, state) => {
                let mut msg = None;
                for (k, chn) in self.chains.iter_mut() {
                    if chn.set_fallthru(uu, state) {
                        msg = Some(Message::ChainFallthru(k.clone(), chn.fallthru.clone()));
                        break;
                    }
                }
                if let Some(msg) = msg {
                    self.send(msg);
                }
            },
            Message::Go(ct) => {
                let mut chn = self.chains.remove(&ct).unwrap();
                chn.go(self, evl);
                self.chains.insert(ct, chn);
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
