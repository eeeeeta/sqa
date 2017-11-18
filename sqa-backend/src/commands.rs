//! Handling processing of server commands.
use codec::{Command, Reply};
use handlers::ReplyData;
use actions::{Action};
use save::Savefile;
use waveform::WaveformContext;
use state::{Context, CD};
use errors::*;
pub fn process_command(ctx: &mut Context, d: &mut CD, c: Command, rd: ReplyData) -> BackendResult<()> {
    use self::Command::*;
    use self::Reply::*;
    match c {
        Ping => {
            d.respond(&rd, Pong)?;
        },
        Version => {
            d.respond(&rd, ServerVersion { ver: super::VERSION.into() })?;
        },
        Subscribe => {
            d.subscribe(&rd);
            d.respond(&rd, Subscribed)?;
        },
        SubscribeAndAssociate { addr } => {
            d.subscribe(&rd);
            d.respond(&rd, Subscribed)?;
            let res = d.associate(&rd, addr).map_err(|e| e.to_string());
            d.respond(&rd, Associated { res })?;
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
            let act = ctx.create_action(&ty, pars, met, old_uu);
            d.respond(&rd, Reply::ActionCreated {
                res: act.map_err(|e| e.to_string())
            })?;
            if broadcast {
                ctx.on_all_actions_changed(d);
            }
        },
        ActionInfo { uuid } => {
            let res = do_with_ctx!(ctx, uuid, |a: &mut Action| {
                a.get_data(ctx).map_err(|e| e.to_string())
            });
            d.respond(&rd, ActionInfoRetrieved { uuid, res })?;
        },
        UpdateActionParams { uuid, params, .. } => {
            let res = do_with_ctx!(ctx, uuid, |a: &mut Action| {
                a.set_params(params, ctx, &d.int_sender).map_err(|e| e.to_string())
            });
            d.respond(&rd, ActionParamsUpdated { uuid, res })?;
        },
        UpdateActionMetadata { uuid, meta } => {
            let res = do_with_ctx!(ctx, uuid, |a: &mut Action| {
                Ok(a.set_meta(meta))
            });
            d.respond(&rd, ActionMetadataUpdated { uuid, res })?;
        },
        LoadAction { uuid } => {
            let res = do_with_ctx!(ctx, uuid, |a: &mut Action| {
                a.load(ctx, &d.int_sender).map_err(|e| e.to_string())
            });
            d.respond(&rd, ActionLoaded { uuid, res })?;
        },
        ResetAction { uuid } => {
            let res = do_with_ctx!(ctx, uuid, |a: &mut Action| {
                a.reset(ctx, &d.int_sender);
                Ok(())
            });
            d.respond(&rd, ActionReset { uuid, res })?;
        },
        PauseAction { uuid } => {
            let res = do_with_ctx!(ctx, uuid, |a: &mut Action| {
                a.pause(ctx, &d.int_sender);
                Ok(())
            });
            d.respond(&rd, ActionMaybePaused { uuid, res })?;
        },
        ExecuteAction { uuid } => {
            let res = do_with_ctx!(ctx, uuid, |a: &mut Action| {
                a.execute(::sqa_engine::Sender::<()>::precise_time_ns(), ctx, &d.int_sender)
                    .map_err(|e| e.to_string())
            });
            d.respond(&rd, ActionExecuted { uuid, res })?;
        },
        ActionList => {
            ctx.on_all_actions_changed(d);
        },
        DeleteAction { uuid } => {
            if ctx.actions.remove(uuid).is_some() {
                d.respond(&rd, ActionDeleted { uuid, deleted: true })?;
                d.broadcast(UpdateActionDeleted { uuid })?;
                ctx.on_all_actions_changed(d);
            }
            else {
                d.respond(&rd, ActionDeleted { uuid, deleted: false })?;
            }
        },
        ReorderAction { uuid, new_pos } => {
            let res = ctx.actions.reorder(uuid, new_pos).map_err(|e| e.to_string());
            d.respond(&rd, ActionReordered { uuid, res })?;
        },
        GetMixerConf => {
            d.respond(&rd, UpdateMixerConf { conf: ctx.mixer.obtain_config() })?;
        },
        SetMixerConf { conf } => {
            d.respond(&rd, MixerConfSet {res: ctx.mixer.process_config(conf)
                                         .map_err(|e| e.to_string())})?;
            d.respond(&rd, UpdateMixerConf { conf: ctx.mixer.obtain_config() })?;
        },
        MakeSavefile { save_to } => {
            let res = Savefile::save_to_file(ctx, &save_to);
            d.respond(&rd, SavefileMade { res: res.map_err(|e| e.to_string()) })?;
        },
        LoadSavefile { load_from, force } => {
            let res = Savefile::apply_from_file(ctx, &load_from, Some(d), force);
            d.respond(&rd, SavefileLoaded { res: res.map_err(|e| e.to_string()) })?;
        },
        GetUndoState => {
            d.respond(&rd, ReplyUndoState { ctx: ctx.undo.state() })?;
        },
        Undo => {
            if let Some(cmd) = ctx.undo.undo() {
                ctx.on_undo_changed(d);
                process_command(ctx, d, cmd, rd)?;
            }
        },
        Redo => {
            if let Some(cmd) = ctx.undo.redo() {
                ctx.on_undo_changed(d);
                process_command(ctx, d, cmd, rd)?;
            }
        },
        GenerateWaveform { uuid, req } => {
            WaveformContext::execute_request(ctx, d, uuid, req)?;
        }
        _ => {}
    };
    Ok(())
}
