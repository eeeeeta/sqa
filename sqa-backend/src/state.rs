//! Handling the global state of the backend.

use tokio_core::reactor::Remote;
use uuid::Uuid;
use handlers::{ConnHandler, ConnData, ReplyData};
use codec::{Command, Reply};
use std::collections::HashMap;
use actions::{Action, ActionParameters, ActionMetadata, PlaybackState};
use sqa_engine::sync::{AudioThreadMessage};
use sqa_ffmpeg::MediaContext;
use mixer::{MixerContext};
use undo::{self, UndoContext};
use waveform::WaveformContext;
use errors::*;
use handlers;
use commands;
use tokio_core::reactor::Handle;
use action_manager::ActionManager;
pub struct Context {
    pub remote: Remote,
    pub mixer: MixerContext,
    pub media: MediaContext,
    pub undo: UndoContext,
    pub waveform: WaveformContext,
    pub actions: ActionManager,
    pub sender: Option<IntSender>,
    pub handle: Option<Handle>,
}
pub enum ServerMessage {
    Audio(AudioThreadMessage),
    ActionStateChange(Uuid, PlaybackState),
    ActionWarning(Uuid, String),
}

pub type IntSender = handlers::IntSender<ServerMessage>;
pub type CD = ConnData<ServerMessage>;

impl ConnHandler for Context {
    type Message = ServerMessage;
    fn init(&mut self, d: &mut CD) {
        self.mixer.start_messaging(d.int_sender.clone());
        self.sender = Some(d.int_sender.clone());
        self.handle = Some(d.handle.clone());
    }
    fn wakeup(&mut self, d: &mut CD) {
        WaveformContext::on_wakeup(self, d).unwrap();
        ActionManager::on_wakeup(self, d)
    }
    fn internal(&mut self, d: &mut CD, m: ServerMessage) {
        match m {
            ServerMessage::Audio(msg) => {
                use self::AudioThreadMessage::*;
                match msg {
                    PlayerAdded(uu) => debug!("player added: {}", uu),
                    PlayerRejected(ref p) => warn!("player rejected: {}", p.uuid),
                    PlayerRemoved(ref p) => debug!("player removed: {}", p.uuid),
                    PlayerInvalidOutpatch(uu) => trace!("player has invalid outpatch: {}", uu),
                    PlayerBufHalf(uu) => trace!("player buf at half: {}", uu),
                    PlayerBufEmpty(uu) => warn!("player buf at empty: {}", uu),
                    Xrun => warn!("audio thread xrun")
                }
                for (uu, mut act) in self.actions.remove_all_for_editing().into_iter() {
                    act.accept_audio_message(self, &d.int_sender, &msg);
                    self.actions.insert_after_editing(uu, act);
                }
            },
            ServerMessage::ActionStateChange(uu, ps) => {
                if let Some(mut act) = self.actions.remove_for_editing(uu, false) {
                    if let Err(e) = act.state_change(ps, self, &d.int_sender) {
                        warn!("failed state change: {:?}", e);
                    }
                    self.on_action_changed(d, &mut act);
                    self.actions.insert_after_editing(uu, act);
                }
            },
            _ => {}
        }
    }
    fn external(&mut self, d: &mut CD, c: Command, rd: ReplyData) -> BackendResult<()> {
        if let Some(ch) = undo::cmd_as_undoable(self, &c) {
            self.undo.register_change(ch);
            self.on_undo_changed(d);
        }
        let res = commands::process_command(self, d, c, rd);
        for changed in self.actions.clear_changed() {
            let _: BackendResult<()> = do_with_ctx!(self, changed, |a: &mut Action| {
                self.on_action_changed(d, a);
                Ok(())
            }, false);
        }
        if self.actions.clear_order_changed() {
            self.on_order_changed(d);
        }
        res
    }
}
impl Context {
    pub fn new(r: Remote) -> Self {
        let mut ctx = Context {
            remote: r,
            mixer: MixerContext::new().unwrap(),
            media: ::sqa_ffmpeg::init().unwrap(),
            undo: UndoContext::new(),
            actions: ActionManager::new(),
            waveform: WaveformContext::new(),
            sender: None,
            handle: None,
        };
        ctx.mixer.default_config().unwrap();
        ctx
    }
    pub fn sender(&self) -> &IntSender {
        self.sender.as_ref().unwrap()
    }
    pub fn make_action_list(&mut self) -> Reply {
        let mut resp = HashMap::new();
        for uu in self.actions.action_list() {
            let _: Result<(), String> = do_with_ctx!(self, uu, |a: &mut Action| {
                if let Ok(data) = a.get_data(self) {
                    resp.insert(uu, data);
                }
                else {
                    error!("FIXME: handle failure to get_data");
                }
                Ok(())
            });
        }
        let order = self.actions.order().clone();
        Reply::ReplyActionList { list: resp, order }
    }
    pub fn on_action_changed(&mut self, d: &mut CD, action: &mut Action) {
        if let Ok(data) = action.get_data(self) {
            if let Err(e) = d.broadcast(Reply::UpdateActionInfo {
                uuid: action.uuid(),
                data
            }) {
                error!("fixme: error in on_action_changed: {:?}", e);
            }
        }
    }
    pub fn on_all_actions_changed(&mut self, d: &mut CD) {
        let rpl = self.make_action_list();
        if let Err(e) = d.broadcast(rpl) {
            error!("fixme: error in on_all_actions_changed: {:?}", e);
        }
    }
    pub fn on_order_changed(&mut self, d: &mut CD) {
        let order = self.actions.order();
        if let Err(e) = d.broadcast(Reply::UpdateOrder { order: order.clone() }) {
            error!("fixme: error in on_order_changed: {:?}", e);
        }
    }
    pub fn on_undo_changed(&mut self, d: &mut CD) {
        let ctx = self.undo.state();
        if let Err(e) = d.broadcast(Reply::ReplyUndoState { ctx }) {
            error!("fixme: error in on_undo_changed: {:?}", e);
        }
    }
    pub fn create_action(&mut self, ty: &str, pars: Option<ActionParameters>, met: Option<ActionMetadata>, old_uu: Option<Uuid>) -> BackendResult<Uuid> {
        let mut act = match &*ty {
            "audio" => Action::audio(),
            "fade" => Action::fade(),
            x => bail!("Unknown action type: {}", x)
        };
        if let Some(uu) = old_uu {
            if self.actions.get(&uu).is_some() {
                bail!("UUID {} already exists!", uu);
            }
        }
        if let Some(uu) = old_uu {
            act.set_uuid(uu);
        }
        if let Some(ref met) = met {
            act.set_meta(met.clone());
        }
        let uu = act.uuid();
        if let Some(ref pars) = pars {
            // FIXME: we should ideally send something here
            let sender = self.sender.clone().unwrap();
            if let Err(e) = act.set_params(pars.clone(), self, &sender) {
                error!("fixme: set_params failed in create_action: {:?}", e);
            }
        }
        self.actions.mark_changed(uu);
        self.actions.insert(uu, act);
        Ok(uu)
    }
}
