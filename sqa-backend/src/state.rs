//! Handling the global state of the backend.

use tokio_core::reactor::Remote;
use std::any::Any;
use uuid::Uuid;
use handlers::{ConnHandler, ConnData};
use codec::{Command, Reply};
use std::collections::HashMap;
use actions::{Action, PlaybackState};
use sqa_engine::sync::{AudioThreadMessage};
use sqa_ffmpeg::MediaContext;
use mixer::{MixerContext};
use errors::*;
use std::mem;
pub struct Context {
    pub remote: Remote,
    pub mixer: MixerContext,
    pub media: MediaContext,
    pub actions: HashMap<Uuid, Action>
}
macro_rules! do_with_ctx {
    ($self:expr, $uu:expr, $clo:expr) => {{
        match $self.actions.remove($uu) {
            Some(mut a) => {
                let ret = $clo(&mut a);
                $self.actions.insert(*$uu, a);
                ret
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
                for (uu, mut act) in mem::replace(&mut self.actions, HashMap::new()).into_iter() {
                    act.accept_audio_message(self, &d.internal_tx, &msg);
                    self.actions.insert(uu, act);
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
    fn external(&mut self, d: &mut CD, c: Command) -> BackendResult<()> {
        use self::Command::*;
        use self::Reply::*;
        match c {
            Ping => {
                d.respond(Pong)?;
            },
            Version => {
                d.respond(ServerVersion { ver: super::VERSION.into() })?;
            },
            Subscribe => {
                d.subscribe();
                d.respond(Subscribed)?;
            },
            x @ CreateAction { .. } | x @ CreateActionWithParams { .. } => {
                let ty;
                let mut pars = None;
                match x {
                    CreateAction { typ } => ty = typ,
                    CreateActionWithParams { typ, params } => {
                        ty = typ;
                        pars = Some(params);
                    },
                    _ => unreachable!()
                }
                let act = match &*ty {
                    "audio" => Ok(Action::new_audio()),
                    "fade" => Ok(Action::new_fade()),
                    _ => Err("Unknown action type".into())
                };
                let act = act.map(|mut act| {
                    let uu = act.uuid();
                    if let Some(ref pars) = pars {
                        act.set_params(pars.clone(), self);
                    }
                    self.on_action_changed(d, &mut act);
                    self.actions.insert(uu, act);
                    uu
                });
                d.respond(ActionCreated {
                    res: act
                })?;
            },
            ActionInfo { uuid } => {
                let res = do_with_ctx!(self, &uuid, |a: &mut Action| {
                    let ret = a.get_data(self).map_err(|e| e.to_string());
                    self.on_action_changed(d, a);
                    ret
                });
                d.respond(ActionInfoRetrieved { uuid, res })?;
            },
            UpdateActionParams { uuid, params } => {
                let res = do_with_ctx!(self, &uuid, |a: &mut Action| {
                    let ret = a.set_params(params, self).map_err(|e| e.to_string());
                    self.on_action_changed(d, a);
                    ret
                });
                d.respond(ActionParamsUpdated { uuid, res })?;
            },
            LoadAction { uuid } => {
                let res = do_with_ctx!(self, &uuid, |a: &mut Action| {
                    let ret = a.load(self, &d.internal_tx).map_err(|e| e.to_string());
                    self.on_action_changed(d, a);
                    ret
                });
                d.respond(ActionLoaded { uuid, res })?;
            },
            ExecuteAction { uuid } => {
                let res = do_with_ctx!(self, &uuid, |a: &mut Action| {
                    let ret = a.execute(::sqa_engine::Sender::<()>::precise_time_ns(), self, &d.internal_tx).map_err(|e| e.to_string());
                    self.on_action_changed(d, a);
                    ret
                });
                d.respond(ActionExecuted { uuid, res })?;
            },
            ActionList => {
                let mut resp = HashMap::new();
                for (uu, mut act) in mem::replace(&mut self.actions, HashMap::new()).into_iter() {
                    if let Ok(data) = act.get_data(self) {
                        resp.insert(uu, data);
                    }
                    else {
                        println!("FIXME: handle failure to get_data");
                    }
                    self.actions.insert(uu, act);
                }
                d.respond(ReplyActionList { list: resp })?;
            },
            DeleteAction { uuid } => {
                if self.actions.remove(&uuid).is_some() {
                    d.respond(ActionDeleted { uuid, deleted: true })?;
                    d.broadcast(UpdateActionDeleted { uuid })?;
                }
                else {
                    d.respond(ActionDeleted { uuid, deleted: false })?;
                }
            },
            GetMixerConf => {
                d.respond(UpdateMixerConf { conf: self.mixer.obtain_config() })?;
            },
            SetMixerConf { conf } => {
                d.respond(MixerConfSet {res: self.mixer.process_config(conf)
                                        .map_err(|e| e.to_string())})?;
                d.respond(UpdateMixerConf { conf: self.mixer.obtain_config() })?;
            },
            _ => {}
        };
        Ok(())
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
    pub fn on_action_changed(&mut self, d: &mut CD, action: &mut Action) {
        if let Ok(data) = action.get_data(self) {
            d.broadcast(Reply::UpdateActionInfo {
                    uuid: action.uuid(),
                    data
            });
        }
    }
}
