//! Saving and loading support.

use uuid::Uuid;
use actions::{ActionMetadata, ActionParameters, OpaqueAction};
use codec::Reply;
use mixer::MixerConf;
use std::collections::HashMap;
use errors::*;
use state::{CD, Context};
use undo::UndoContext;
use std::mem;
use rmp_serde;
use std::fs::File;
use std::io::{Read, Write};

pub static SAVEFILE_VERSION: &str = "indev";

#[derive(Serialize, Deserialize)]
pub struct SavedAction {
    typ: String,
    meta: ActionMetadata,
    params: ActionParameters
}
impl From<OpaqueAction> for SavedAction {
    fn from(opa: OpaqueAction) -> SavedAction {
        let typ = opa.typ().to_string();
        let OpaqueAction { meta, params, .. } = opa;
        SavedAction { typ, meta, params }
    }
}
#[derive(Serialize, Deserialize)]
pub struct Savefile {
    ver: String,
    actions: HashMap<Uuid, SavedAction>,
    mixer_conf: MixerConf,
    undo: UndoContext
}
impl Savefile {
    pub fn save_to_file(ctx: &mut Context, path: &str) -> BackendResult<()> {
        let mut file = File::create(path)?;
        let data = Self::new_from_ctx(ctx)?;
        let data = rmp_serde::to_vec(&data)?;
        file.write_all(&data)?;
        Ok(())
    }
    pub fn apply_from_file(ctx: &mut Context, path: &str, d: Option<&mut CD>, force: bool) -> BackendResult<()> {
        let mut file = File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let mut data: Self = rmp_serde::from_slice(&buf)?;
        data.apply_to_ctx(ctx, d, force)?;
        Ok(())
    }
    pub fn new_from_ctx(ctx: &mut Context) -> BackendResult<Self> {
        let mut _actions = HashMap::new();
        for (uu, mut act) in mem::replace(&mut ctx.actions, HashMap::new()).into_iter() {
            let data = act.get_data(ctx);
            ctx.actions.insert(uu, act);
            _actions.insert(uu, data);
        }
        let mut actions = HashMap::new();
        for (uu, data) in _actions {
            actions.insert(uu, data?.into());
        }
        let mixer_conf = ctx.mixer.obtain_config();
        let undo = ctx.undo.clone();
        let ver = SAVEFILE_VERSION.into();
        Ok(Self { ver, actions, mixer_conf, undo })
    }
    pub fn apply_to_ctx(&mut self, ctx: &mut Context, mut d: Option<&mut CD>, force: bool) -> BackendResult<()> {
        if self.ver != SAVEFILE_VERSION && !force {
            bail!("Savefile version mismatch: our version is {}, but the savefile was saved with {}.", SAVEFILE_VERSION, self.ver);
        }
        ctx.actions = HashMap::new();
        if let Some(ref mut d) = d {
            let resp = ctx.refresh_action_list();
            d.broadcast(Reply::ReplyActionList { list: resp })?;
        }
        for (uu, sa) in self.actions.iter_mut() {
            let res = ctx.create_action(&sa.typ, Some(sa.params.clone()), Some(sa.meta.clone()), Some(*uu));
            if !force {
                let _ = res?;
            }
        }
        let res = ctx.mixer.process_config(self.mixer_conf.clone());
        if !force {
            let _ = res?;
        }
        ctx.undo = self.undo.clone();
        if let Some(ref mut d) = d {
            ctx.on_all_actions_changed(d);
            ctx.on_undo_changed(d);
        }
        Ok(())
    }
}
