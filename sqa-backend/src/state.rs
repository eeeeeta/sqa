//! Handling the global state of the backend.

use tokio_core::reactor::Remote;
use std::any::Any;
use uuid::Uuid;
use handlers::{ConnHandler, ConnData};
use codec::{Command};
use rosc::OscMessage;
use std::net::SocketAddr;
use sqa_engine::EngineContext;
use std::collections::HashMap;
use actions::{Action, PlaybackState};
use sqa_engine::sync::{AudioThreadMessage, AudioThreadHandle};
use sqa_ffmpeg::MediaContext;
use actions::ParameterError;
pub struct Context {
    pub remote: Remote,
    pub engine: EngineContext,
    pub media: MediaContext,
    pub actions: HashMap<Uuid, Action>
}
pub struct ActionContext<'a> {
    pub engine: &'a mut EngineContext,
    pub media: &'a mut MediaContext,
    pub remote: &'a mut Remote
}
pub enum ServerMessage {
    Audio(AudioThreadMessage),
    ActionStateChange(Uuid, PlaybackState),
    ActionLoad(Uuid, Box<Any>),
    ActionPanic(Uuid, Box<::std::error::Error>),
    ActionCustom(Uuid, Box<Any>),
}
pub type IntSender = ::futures::sync::mpsc::Sender<ServerMessage>;
type CD = ConnData<ServerMessage>;
impl ConnHandler for Context {
    type Message = ServerMessage;
    fn internal(&mut self, _: &mut CD, m: ServerMessage) {
        match m {
            ServerMessage::Audio(mut msg) => {
                for (_, act) in self.actions.iter_mut() {
                    if act.accept_audio_message(&mut msg) {
                        break;
                    }
                }
            },
            ServerMessage::ActionStateChange(uu, ps) => {
                if let Some(act) = self.actions.get_mut(&uu) {
                    act.state_change(ps);
                }
            },
            ServerMessage::ActionCustom(uu, msg) => {
                if let Some(act) = self.actions.get_mut(&uu) {
                    act.message(msg);
                }
            },
            _ => {}
        }
    }
    fn external(&mut self, d: &mut CD, c: Command) {
        use self::Command::*;
        match c {
            Ping => {
                d.respond("/pong".into());
            },
            CreateAction { typ } => {
                d.reply::<Result<Uuid, String>>(match &*typ {
                    "audio" => {
                        let act = Action::new_audio();
                        let uu = act.uuid();
                        self.actions.insert(uu, act);
                        Ok(uu)
                    },
                    _ => Err("Unknown action type".into())
                });
            },
            ActionParams { uuid } => {
                d.reply::<Result<String, String>>(match self.actions.get(&uuid) {
                    Some(a) => {
                        a.get_params().map_err(|e| e.to_string())
                    },
                    _ => Err("No action found".into())
                });
            },
            UpdateActionParams { uuid, params } => {
                d.reply::<Result<(), String>>(match self.actions.get_mut(&uuid) {
                    Some(a) => {
                        a.set_params(&params).map_err(|e| e.to_string())
                    },
                    _ => Err("No action found".into())
                });
            },
            VerifyAction { uuid } => {
                d.reply::<Result<Vec<ParameterError>, String>>({
                   match self.actions.get_mut(&uuid) {
                       Some(a) => {
                            let ctx = ActionContext {
                                engine: &mut self.engine,
                                media: &mut self.media,
                                remote: &mut self.remote
                            };
                            Ok(a.verify_params(ctx))
                        },
                        _ => Err("No action found".into())
                    }
                });
            },
            LoadAction { uuid } => {
                let x = {
                   match self.actions.get_mut(&uuid) {
                       Some(a) => {
                            let ctx = ActionContext {
                                engine: &mut self.engine,
                                media: &mut self.media,
                                remote: &mut self.remote
                            };
                            a.load(ctx, &d.internal_tx).map_err(|e| e.to_string())
                        },
                        _ => Err("No action found".into())
                    }
                };
                d.reply::<Result<bool, String>>(x);
            },
            ExecuteAction { uuid } => {
                let x = {
                   match self.actions.get_mut(&uuid) {
                       Some(a) => {
                            let ctx = ActionContext {
                                engine: &mut self.engine,
                                media: &mut self.media,
                                remote: &mut self.remote
                            };
                            a.execute(::sqa_engine::Sender::<()>::precise_time_ns(), ctx, &d.internal_tx).map_err(|e| e.to_string())
                        },
                        _ => Err("No action found".into())
                    }
                };
                d.reply::<Result<bool, String>>(x);
            },
            DeleteAction { uuid } => {
                d.reply::<bool>(self.actions.remove(&uuid).is_some());
            },
            _ => {}
        }
    }
}
impl Context {
    pub fn new(r: Remote) -> Self {
        let mut ctx = Context {
            remote: r,
            engine: EngineContext::new(Some("SQA Backend alpha")).unwrap(),
            actions: HashMap::new(),
            media: ::sqa_ffmpeg::init().unwrap()
        };
        for (i, port) in ctx.engine.conn.get_ports(None, None, Some(::sqa_engine::sqa_jack::PORT_IS_INPUT | ::sqa_engine::sqa_jack::PORT_IS_PHYSICAL)).unwrap().into_iter().enumerate() {
            let st = format!("channel {}", i);
            ctx.engine.new_channel(&st).unwrap();
            ctx.engine.conn.connect_ports(&ctx.engine.chans[i], &port).unwrap();
        }
        ctx
    }
}
