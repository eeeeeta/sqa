//! Managing actions.
use actions::{Action};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use state::{Context, CD};
use std::mem;
use errors::*;

macro_rules! do_with_ctx {
    ($ctx:expr, $uu:expr, $clo:expr, $mk:expr) => {{
        match $ctx.actions.remove_for_editing($uu, $mk) {
            Some(mut a) => {
                let ret = $clo(&mut a);
                $ctx.actions.insert_after_editing($uu, a);
                ret
            },
            _ => Err("No action found".into())
        }
    }};
    ($ctx:expr, $uu:expr, $clo:expr) => {{
        do_with_ctx!($ctx, $uu, $clo, true)
    }}
}


#[derive(Default)]
pub struct ActionManager {
    actions: HashMap<Uuid, Action>,
    async_actions: HashSet<Uuid>,
    changed: HashSet<Uuid>,
    order: Vec<Uuid>,
    order_changed: bool
}
impl ActionManager {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn get(&self, uuid: &Uuid) -> Option<&Action> {
        self.actions.get(uuid)
    }
    pub fn get_mut(&mut self, uuid: &Uuid) -> Option<&mut Action> {
        self.actions.get_mut(uuid)
    }
    pub fn order(&self) -> &Vec<Uuid> {
        &self.order
    }
    pub fn position_of(&self, uuid: Uuid) -> Option<usize> {
        self.order.iter().position(|&uu| uu == uuid)
    }
    pub fn restore_order(&mut self, order: Vec<Uuid>) -> BackendResult<()> {
        debug!("attempting to restore order...");
        for uu in order.iter() {
            if self.actions.get(&uu).is_none() {
                bail!("UUID {} is mentioned in order, but doesn't exist", uu);
            }
        }
        self.order = order;
        debug!("order restored");
        Ok(())
    }
    pub fn action_list(&self) -> Vec<Uuid> {
        self.actions.iter().map(|(x, _)| x.clone()).collect::<Vec<_>>()
    }
    pub fn insert(&mut self, uuid: Uuid, act: Action) {
        debug!("creating new action {}", uuid);
        assert!(self.actions.get(&uuid).is_none());
        assert!(self.position_of(uuid).is_none());
        self.actions.insert(uuid, act);
        self.order.push(uuid);
        self.order_changed = true;
    }
    pub fn insert_with_order(&mut self, uuid: Uuid, act: Action, mut order: usize) {
        debug!("creating new action {} with order {}", uuid, order);
        if order > self.order.len() {
            order = self.order.len();
        }
        self.actions.insert(uuid, act);
        self.order.insert(order, uuid);
        self.order_changed = true;
    }
    pub fn reorder(&mut self, uuid: Uuid, new_pos: usize) -> BackendResult<()> {
        let pos = self.position_of(uuid).ok_or("UUID not present in order")?;
        if new_pos >= self.order.len() {
            bail!("New position is out of bounds");
        }
        debug!("reordering action {} to position {}", uuid, new_pos);
        self.order.remove(pos);
        self.order.insert(new_pos, uuid);
        self.order_changed = true;
        Ok(())
    }
    pub fn remove(&mut self, uuid: Uuid) -> Option<Action> {
        debug!("removing action {}", uuid);
        if let Some(pos) = self.position_of(uuid) {
            self.order.remove(pos);
            self.order_changed = true;
        }
        self.actions.remove(&uuid)
    }
    pub fn remove_for_editing(&mut self, uuid: Uuid, mark_changed: bool) -> Option<Action> {
        if mark_changed {
            self.mark_changed(uuid);
        }
        self.actions.remove(&uuid)
    }
    pub fn insert_after_editing(&mut self, uuid: Uuid, act: Action) {
        assert!(self.actions.get(&uuid).is_none());
        self.actions.insert(uuid, act);
    }
    pub fn remove_all_for_editing(&mut self) -> HashMap<Uuid, Action> {
        mem::replace(&mut self.actions, HashMap::new())
    }
    pub fn clear_changed(&mut self) -> HashSet<Uuid> {
        mem::replace(&mut self.changed, HashSet::new())
    }
    pub fn clear_order_changed(&mut self) -> bool {
        if self.order_changed {
            trace!("order was changed");
        }
        mem::replace(&mut self.order_changed, false)
    }
    pub fn mark_changed(&mut self, uu: Uuid) {
        trace!("UUID {} was changed", uu);
        self.changed.insert(uu);
    }
    pub fn register_interest(&mut self, uu: Uuid) {
        trace!("UUID {} registers interest", uu);
        self.async_actions.insert(uu);
    }
    pub fn unregister_interest(&mut self, uu: Uuid) {
        trace!("UUID {} deregisters interest", uu);
        self.async_actions.remove(&uu);
    }
    pub fn on_wakeup(ctx: &mut Context, d: &mut CD) {
        let mut to_remove = vec![];
        for uu in ctx.actions.async_actions.clone() {
            let continue_polling =
                if let Some(mut act) = ctx.actions.remove_for_editing(uu, false) {
                    let a = act.poll(ctx, &d.int_sender);
                    ctx.on_action_changed(d, &mut act);
                    ctx.actions.insert_after_editing(uu, act);
                    a
                }
            else {
                false
            };
            if !continue_polling {
                to_remove.push(uu);
            }
        }
        for uu in to_remove {
            ctx.actions.async_actions.remove(&uu);
        }
    }
}
