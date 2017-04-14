//! Handling the global state of the backend.

use tokio_core::reactor::Remote;
use std::any::Any;
use uuid::Uuid;
use handlers::{ConnHandler, ConnData};
use codec::{Command, Reply};
use rosc::{OscMessage, OscType};
use std::net::SocketAddr;
use sqa_engine::EngineContext;
use std::collections::HashMap;
use actions::{Action, PlaybackState};
use sqa_engine::sync::{AudioThreadMessage, AudioThreadHandle};
use sqa_ffmpeg::MediaContext;
use actions::ParameterError;
use mixer::{MixerConf, MixerContext};
pub struct Context {
    pub remote: Remote,
    pub mixer: MixerContext,
    pub media: MediaContext,
    pub actions: HashMap<Uuid, Action>
}
pub struct ActionContext<'a> {
    pub mixer: &'a mut MixerContext,
    pub media: &'a mut MediaContext,
    pub remote: &'a mut Remote
}
macro_rules! ctx_from_self {
    ($self:expr) => {
        ActionContext {
            mixer: &mut $self.mixer,
            media: &mut $self.media,
            remote: &mut $self.remote
        }
    }
}
macro_rules! do_with_ctx {
    ($self:expr, $uu:expr, $clo:expr) => {{
        match $self.actions.get_mut($uu) {
            Some(a) => {
                let ctx = ctx_from_self!($self);
                $clo(a, ctx)
            },
            _ => Err("No action found".into())
        }
    }}
}
pub enum ServerMessage {
    Audio(AudioThreadMessage),
    ActionStateChange(Uuid, PlaybackState),
    ActionLoad(Uuid, Box<Any+Send>),
    ActionCustom(Uuid, Box<Any+Send>),
    ActionWarning(Uuid, String)
}
pub type IntSender = ::futures::sync::mpsc::Sender<ServerMessage>;
type CD = ConnData<ServerMessage>;
impl ConnHandler for Context {
    type Message = ServerMessage;
    fn init(&mut self, d: &mut CD) {
        self.mixer.start_messaging(d.internal_tx.clone());
    }
    fn internal(&mut self, d: &mut CD, m: ServerMessage) {
        match m {
            ServerMessage::Audio(msg) => {
                for (_, act) in self.actions.iter_mut() {
                    let mut ctx = ctx_from_self!(self);
                    if act.accept_audio_message(&mut ctx, &d.internal_tx, &msg) {
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
                d.respond(Reply::Pong.into());
            },
            Version => {
                d.respond(Reply::ServerVersion { ver: super::VERSION.into() }.into());
            },
            Subscribe => {
                d.subscribe();
                d.respond(Reply::Subscribed.into());
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
            ActionInfo { uuid } => {
                d.reply::<Result<::serde_json::Value, String>>(
                    do_with_ctx!(self, &uuid, |a: &mut Action, mut ctx: ActionContext| {
                        a.get_data(&mut ctx).map_err(|e| e.to_string())
                    })
                );
            },
            UpdateActionParams { uuid, params } => {
                let x = do_with_ctx!(self, &uuid, |a: &mut Action, mut ctx: ActionContext| {
                    let ret = a.set_params(&params).map_err(|e| e.to_string());
                    Self::on_action_changed(d, a, &mut ctx);
                    ret
                });
                d.reply::<Result<(), String>>(x);
            },
            LoadAction { uuid } => {
                let x = do_with_ctx!(self, &uuid, |a: &mut Action, mut ctx: ActionContext| {
                    let ret = a.load(&mut ctx, &d.internal_tx).map_err(|e| e.to_string());
                    Self::on_action_changed(d, a, &mut ctx);
                    ret
                });
                d.reply::<Result<(), String>>(x);
            },
            ExecuteAction { uuid } => {
                let x = do_with_ctx!(self, &uuid, |a: &mut Action, mut ctx: ActionContext| {
                    let ret = a.execute(::sqa_engine::Sender::<()>::precise_time_ns(), &mut ctx, &d.internal_tx).map_err(|e| e.to_string());
                    Self::on_action_changed(d, a, &mut ctx);
                    ret
                });
                d.reply::<Result<(), String>>(x);
            },
            DeleteAction { uuid } => {
                d.reply::<bool>(self.actions.remove(&uuid).is_some());
            },
            GetMixerConf => {
                d.reply::<MixerConf>(self.mixer.obtain_config());
            },
            SetMixerConf { conf } => {
                d.reply::<Result<(), String>>(self.mixer.process_config(conf)
                                              .map_err(|e| e.to_string()));
            },
            _ => {}
        }
    }
}
impl Context {
    pub fn new(r: Remote) -> Self {
        let mut ctx = Context {
            remote: r,
            mixer: MixerContext::new().unwrap(),
            actions: HashMap::new(),
            media: ::sqa_ffmpeg::init().unwrap()
        };
        ctx.mixer.default_config().unwrap();
        ctx
    }
    pub fn on_action_changed(d: &mut CD, action: &mut Action, ctx: &mut ActionContext) {
        d.broadcast::<Result<::serde_json::Value, String>>(
            format!("/update/action/{}", action.uuid()),
            action.get_data(ctx).map_err(|e| e.to_string())
        );
    }
}
