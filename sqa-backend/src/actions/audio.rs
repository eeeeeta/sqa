//! Plays back an audio file.

use sqa_engine::{PlainSender, BufferSender, Sender};
use sqa_ffmpeg::MediaFile;
use super::{ParameterError, ControllerParams, ActionController};
use state::{ServerMessage, ActionContext};
use futures::future;
use futures::Future;
use std::error::Error;
use std::any::Any;
use futures::sync::{mpsc, oneshot};
use std::thread;
use std::panic;
use errors::*;
pub struct Controller {
    params: AudioParams,
    senders: Vec<PlainSender>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AudioParams {
    url: Option<String>
}

impl Controller {
    pub fn new() -> Self {
        Controller {
            params: AudioParams {
                url: None
            },
            senders: vec![]
        }
    }
}
impl ActionController for Controller {
    type Parameters = AudioParams;

    fn desc(&self) -> String {
        format!("Play audio at {}", self.params.url.as_ref().map(|x| x as &str).unwrap_or("???"))
    }
    fn get_params(&self) -> &AudioParams {
        &self.params
    }
    fn set_params(&mut self, p: AudioParams) {
        self.params = p;
    }
    fn verify_params(&self, ctx: &mut ActionContext) -> Vec<ParameterError> {
        let mut ret = vec![];
        if let Some(ref st) = self.params.url {
            let mf = MediaFile::new(ctx.media, &st);
            if let Err(e) = mf {
                ret.push(ParameterError {
                    name: "url".into(),
                    err: format!("Error opening URL: {}", e)
                })
            }
        }
        else {
            ret.push(ParameterError {
                name: "url".into(),
                err: "This field is required.".into()
            })
        }
        ret
    }
    fn load(&mut self, params: ControllerParams) -> BackendResult<bool> {
        let url = self.params.url.clone().unwrap();
        let mut mf = MediaFile::new(params.ctx.media, &url)?;
        let mut senders: Vec<BufferSender> = (0..mf.channels())
            .map(|_| params.ctx.engine.new_sender(mf.sample_rate() as u64))
            .collect();
        for (i, s) in senders.iter_mut().enumerate() {
            if params.ctx.engine.chans.get(i).is_some() {
                s.set_output_patch(i);
            }
        }
        let plains: Vec<PlainSender> = senders.iter()
            .map(|s| s.make_plain())
            .collect();
        thread::spawn(move || {
            for x in &mut mf {
                if let Ok(mut x) = x {
                    for (i, ch) in senders.iter_mut().enumerate() {
                        x.set_chan(i);
                        for smpl in &mut x {
                            ch.buf.push(smpl.f32() * 0.5);
                        }
                    }
                }
            }
        });
        self.senders = plains;
        Ok(true)
    }
    fn execute(&mut self, time: u64, data: Option<Box<Any>>, ctx: ControllerParams) -> BackendResult<bool> {
        for sender in self.senders.iter_mut() {
            sender.set_start_time(time);
            sender.set_active(true);
        }
        Ok(true)
    }
}
