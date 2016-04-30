//! Program state management.

use streamv2::FileStreamX;
use mixer::{QChannel, Magister, Sink, Source, DeviceSink, FRAMES_PER_CALLBACK};
use std::collections::BTreeMap;
use uuid::Uuid;
use portaudio as pa;

/// Global context
pub struct Context<'a> {
    pub idents: BTreeMap<String, Vec<FileStreamX>>,
    pub mstr: Magister<'a>,
    pub qchans: Vec<Uuid>,
    pub qchan_outs: Vec<Uuid>,
    pub outs: Vec<Uuid>
}
impl<'a> Context<'a> {
    pub fn new() -> Self {
        let mut ctx = Context {
            idents: BTreeMap::new(),
            mstr: Magister::new(),
            qchans: Vec::new(),
            qchan_outs: Vec::new(),
            outs: Vec::new()
        };
        for _ in 0..16 {
            let mut qch = QChannel::new(44_100);
            qch.frames_hint(FRAMES_PER_CALLBACK);
            let qchx = qch.get_x();
            ctx.qchans.push(qchx.uuid());
            ctx.qchan_outs.push(qchx.uuid_pair());
            ctx.mstr.add_source(Box::new(qch));
            ctx.mstr.add_sink(Box::new(qchx));
        };
        ctx
    }
}
