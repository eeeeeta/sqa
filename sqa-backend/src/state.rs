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
pub struct Context {
    pub remote: Remote,
    pub engine: EngineContext,
    pub media: MediaContext,
    pub actions: HashMap<Uuid, Action>
}
pub enum ServerMessage {
    Audio(AudioThreadMessage),
    ActionStateChange(Uuid, PlaybackState),
    ActionCustom(Uuid, Box<Any>),
}
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
            }
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
                    _ => Err("stuff you".into())
                });
            },
            _ => {}
        }
    }
}
impl Context {
    pub fn new(r: Remote) -> Self {
        Context {
            remote: r,
            engine: EngineContext::new(Some("SQA Backend alpha")).unwrap(),
            actions: HashMap::new(),
            media: ::sqa_ffmpeg::init().unwrap()
        }
    }
}
