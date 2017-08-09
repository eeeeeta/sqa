//! Handling the global state of the backend.

use tokio_core::reactor::Remote;
use uuid::Uuid;
use handlers::{ConnHandler, ConnData};
use codec::{Command, Reply};
use std::collections::HashMap;
use actions::{Action, OpaqueAction, ActionParameters, ActionMetadata, PlaybackState};
use sqa_engine::sync::{AudioThreadMessage};
use sqa_ffmpeg::MediaContext;
use mixer::{MixerContext};
use undo::{UndoableChange, UndoContext};
use errors::*;
use save::Savefile;
use handlers;
use tokio_core::reactor::Handle;
use action_manager::ActionManager;
pub struct Context {
    pub remote: Remote,
    pub mixer: MixerContext,
    pub media: MediaContext,
    pub undo: UndoContext,
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
        ActionManager::on_wakeup(self, d)
    }
    fn internal(&mut self, d: &mut CD, m: ServerMessage) {
        match m {
            ServerMessage::Audio(msg) => {
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
    fn external(&mut self, d: &mut CD, c: Command) -> BackendResult<()> {
        if let Some(ch) = self.cmd_as_undoable(&c) {
            self.undo.register_change(ch);
            self.on_undo_changed(d);
        }
        let res = self.process_command(d, c);
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
            sender: None,
            handle: None,
        };
        ctx.mixer.default_config().unwrap();
        ctx
    }
    pub fn sender(&self) -> &IntSender {
        self.sender.as_ref().unwrap()
    }
    pub fn process_command(&mut self, d: &mut CD, c: Command) -> BackendResult<()> {
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
            x @ CreateAction { .. } |
            x @ CreateActionWithUuid { .. } |
            x @ CreateActionWithExtras { .. } |
            x @ ReviveAction { .. } => {
                let ty;
                let mut pars = None;
                let mut met = None;
                let mut old_uu = None;
                match x {
                    CreateAction { typ } => ty = typ,
                    CreateActionWithUuid { typ, uuid } => {
                        ty = typ;
                        old_uu = Some(uuid);
                    },
                    CreateActionWithExtras { typ, params, uuid } => {
                        ty = typ;
                        old_uu = Some(uuid);
                        pars = Some(params);
                    },
                    ReviveAction { uuid, typ, params, meta } => {
                        old_uu = Some(uuid);
                        ty = typ;
                        pars = Some(params);
                        met = Some(meta);
                    },
                    _ => unreachable!()
                }
                let broadcast = met.is_some();
                let act = self.create_action(&ty, pars, met, old_uu);
                d.respond(Reply::ActionCreated {
                    res: act.map_err(|e| e.to_string())
                })?;
                if broadcast {
                    self.on_all_actions_changed(d);
                }
            },
            ActionInfo { uuid } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.get_data(self).map_err(|e| e.to_string())
                });
                d.respond(ActionInfoRetrieved { uuid, res })?;
            },
            UpdateActionParams { uuid, params, .. } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.set_params(params, self, &d.int_sender).map_err(|e| e.to_string())
                });
                d.respond(ActionParamsUpdated { uuid, res })?;
            },
            UpdateActionMetadata { uuid, meta } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    Ok(a.set_meta(meta))
                });
                d.respond(ActionMetadataUpdated { uuid, res })?;
            },
            LoadAction { uuid } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.load(self, &d.int_sender).map_err(|e| e.to_string())
                });
                d.respond(ActionLoaded { uuid, res })?;
            },
            ResetAction { uuid } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.reset(self, &d.int_sender);
                    Ok(())
                });
                d.respond(ActionReset { uuid, res })?;
            },
            PauseAction { uuid } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.pause(self, &d.int_sender);
                    Ok(())
                });
                d.respond(ActionMaybePaused { uuid, res })?;
            },
            ExecuteAction { uuid } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.execute(::sqa_engine::Sender::<()>::precise_time_ns(), self, &d.int_sender)
                        .map_err(|e| e.to_string())
                });
                d.respond(ActionExecuted { uuid, res })?;
            },
            ActionList => {
                self.on_all_actions_changed(d);
            },
            DeleteAction { uuid } => {
                if self.actions.remove(uuid).is_some() {
                    d.respond(ActionDeleted { uuid, deleted: true })?;
                    d.broadcast(UpdateActionDeleted { uuid })?;
                    self.on_all_actions_changed(d);
                }
                else {
                    d.respond(ActionDeleted { uuid, deleted: false })?;
                }
            },
            ReorderAction { uuid, new_pos } => {
                let res = self.actions.reorder(uuid, new_pos).map_err(|e| e.to_string());
                d.respond(ActionReordered { uuid, res })?;
            },
            GetMixerConf => {
                d.respond(UpdateMixerConf { conf: self.mixer.obtain_config() })?;
            },
            SetMixerConf { conf } => {
                d.respond(MixerConfSet {res: self.mixer.process_config(conf)
                                        .map_err(|e| e.to_string())})?;
                d.respond(UpdateMixerConf { conf: self.mixer.obtain_config() })?;
            },
            MakeSavefile { save_to } => {
                let res = Savefile::save_to_file(self, &save_to);
                d.respond(SavefileMade { res: res.map_err(|e| e.to_string()) })?;
            },
            LoadSavefile { load_from, force } => {
                let res = Savefile::apply_from_file(self, &load_from, Some(d), force);
                d.respond(SavefileLoaded { res: res.map_err(|e| e.to_string()) })?;
            },
            GetUndoState => {
                d.respond(ReplyUndoState { ctx: self.undo.state() })?;
            },
            Undo => {
                if let Some(cmd) = self.undo.undo() {
                    self.on_undo_changed(d);
                    self.process_command(d, cmd)?;
                }
            },
            Redo => {
                if let Some(cmd) = self.undo.redo() {
                    self.on_undo_changed(d);
                    self.process_command(d, cmd)?;
                }
            },
            _ => {}
        };
        Ok(())
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
    pub fn cmd_as_undoable(&mut self, cmd: &Command) -> Option<UndoableChange> {
        use self::Command::*;
        match *cmd {
            CreateActionWithUuid { ref typ, uuid } => Some(UndoableChange {
                undo: DeleteAction { uuid },
                redo: CreateActionWithUuid { typ: typ.clone(), uuid },
                desc: format!("create action with type {}", typ)
            }),
            ReviveAction { uuid, ref typ, ref meta, ref params } => Some(UndoableChange {
                undo: DeleteAction { uuid },
                redo: ReviveAction { uuid, typ: typ.clone(), meta: meta.clone(), params: params.clone() },
                desc: format!("revive action of type {}", typ)
            }),
            CreateActionWithExtras { uuid, ref typ, ref params } => Some(UndoableChange {
                undo: DeleteAction { uuid },
                redo: CreateActionWithExtras { uuid, typ: typ.clone(), params: params.clone() },
                desc: format!("create action with type {}", typ)
            }),
            UpdateActionParams { uuid, ref params, ref desc } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.get_data(self).map_err(|e| e.to_string())
                });
                res.ok().map(|old| {
                    UndoableChange {
                        undo: UpdateActionParams { uuid, params: old.params, desc: None },
                        redo: UpdateActionParams { uuid, params: params.clone(), desc: None },
                        desc: desc.as_ref().map(|x| x.clone()).unwrap_or("update action parameters".into())
                    }
                })
            },
            UpdateActionMetadata { uuid, ref meta } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.get_data(self).map_err(|e| e.to_string())
                });
                res.ok().map(|old| {
                    UndoableChange {
                        undo: UpdateActionMetadata { uuid, meta: old.meta },
                        redo: UpdateActionMetadata { uuid, meta: meta.clone() },
                        desc: "update action metadata".into()
                    }
                })
            },
            DeleteAction { uuid } => {
                let res = do_with_ctx!(self, uuid, |a: &mut Action| {
                    a.get_data(self).map_err(|e| e.to_string())
                });
                res.ok().map(|old| {
                    let typ = old.typ().to_string();
                    let OpaqueAction { meta, params, .. } = old;
                    UndoableChange {
                        undo: ReviveAction { uuid, meta, typ, params },
                        redo: DeleteAction { uuid },
                        desc: "delete action".into()
                    }
                })
            },
            SetMixerConf { ref conf } => {
                let old = self.mixer.obtain_config();
                Some(UndoableChange {
                    undo: SetMixerConf { conf: old },
                    redo: SetMixerConf { conf: conf.clone() },
                    desc: "modify mixer configuration".into()
                })
            },
            ReorderAction { uuid, new_pos } => {
                self.actions.position_of(uuid).map(|pos| {
                    UndoableChange {
                        undo: ReorderAction { uuid, new_pos: pos },
                        redo: ReorderAction { uuid, new_pos },
                        desc: "reorder action".into()
                    }
                })
            },
            _ => None
        }
    }
}
