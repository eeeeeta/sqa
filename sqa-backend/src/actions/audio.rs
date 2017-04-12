//! Plays back an audio file.

use sqa_engine::{PlainSender, BufferSender};
use sqa_engine::sync::AudioThreadMessage;
use sqa_ffmpeg::{Frame, MediaFile};
use super::{ParameterError, ControllerParams, PlaybackState, ActionController};
use state::{ServerMessage, ActionContext, IntSender};
use std::thread;
use futures::Sink;
use futures::sink::Wait;
use futures::Future;
use std::time::Duration;
use errors::*;
use std::sync::mpsc::{Sender, Receiver, self};
use uuid::Uuid;

pub enum SpoolerMessage {
    Wakeup,
    Quit
}
pub struct SpoolerContext {
    bsends: Vec<BufferSender>,
    file: MediaFile,
    uuid: Uuid,
    sender: Wait<IntSender>,
    rx: Receiver<SpoolerMessage>,
}
impl SpoolerContext {
    pub fn send_frame(bsends: &mut Vec<BufferSender>, frame: &mut Frame) -> bool {
        let space = bsends[0].buf.capacity() - bsends[0].buf.size();
        for (ch, sample) in frame.take(space * bsends.len()) {
            bsends[ch].buf.try_push(sample.f32());
        }
        !frame.drained()
    }
    pub fn spool(&mut self) {
        let mut current_msg = None;
        let mut current_frame: Option<Frame> = None;
        'outer: loop {
            if let Some(m) = current_msg.take() {
                use self::SpoolerMessage::*;
                match m {
                    Wakeup => {},
                    Quit => return
                }
            }
            if self.bsends[0].buf.size() == self.bsends[0].buf.capacity() {
                current_msg = Some(self.rx.recv().unwrap());
                continue 'outer;
            }
            if let Some(mut frame) = current_frame.take() {
                if Self::send_frame(&mut self.bsends, &mut frame) {
                    current_frame = Some(frame);
                    current_msg = Some(self.rx.recv().unwrap());
                    continue 'outer;
                }
            }
            for frame in &mut self.file {
                match frame {
                    Ok(mut frame) => {
                        if Self::send_frame(&mut self.bsends, &mut frame) {
                            current_frame = Some(frame);
                            current_msg = Some(self.rx.recv().unwrap());
                            continue 'outer;
                        }
                    },
                    Err(e) => {
                        let msg = format!("Spooler error: {:?}", e);
                        self.sender.send(ServerMessage::ActionStateChange(self.uuid,
                                                                          PlaybackState::Errored(msg)))
                            .unwrap();
                        return;
                    }
                }
            }
            // If we got here, we've sent all the frames!
            return;
        }
    }
}
pub struct Controller {
    params: AudioParams,
    senders: Vec<PlainSender>,
    control: Option<Sender<SpoolerMessage>>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AudioParams {
    url: Option<String>,
    patch: Vec<Option<Uuid>>
}

impl Controller {
    pub fn new() -> Self {
        Controller {
            params: AudioParams {
                url: None,
                patch: vec![]
            },
            senders: vec![],
            control: None
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
            match mf {
                Err(e) => {
                    ret.push(ParameterError {
                        name: "url".into(),
                        err: format!("Error opening URL: {}", e)
                    })
                },
                Ok(mf) => {
                    if mf.channels() == 0 {
                        ret.push(ParameterError {
                            name: "url".into(),
                            err: "What sort of file has exactly ZERO channels in it?!".into()
                        });
                    }

                }
            }
            for (i, ch) in self.params.patch.iter().enumerate() {
                if let &Some(ch) = ch {
                    if ctx.mixer.obtain_channel(&ch).is_none() {
                        ret.push(ParameterError {
                            name: "patch".into(),
                            err: format!("Channel {} does not exist.", i)
                        });
                    }
                }
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
        let mf = MediaFile::new(params.ctx.media, &url)?;
        let mut senders: Vec<BufferSender> = (0..mf.channels())
            .map(|_| params.ctx.mixer.new_sender(mf.sample_rate() as u64))
            .collect();
        for (i, s) in senders.iter_mut().enumerate() {
            if let Some(ch) = self.params.patch.get(i) {
                if let &Some(ch) = ch {
                    let ch = params.ctx.mixer.obtain_channel(&ch)
                        .ok_or("One channel mysteriously disappeared")?;
                    s.set_output_patch(ch);
                }
            }
        }
        let plains: Vec<PlainSender> = senders.iter()
            .map(|s| s.make_plain())
            .collect();
        let (tx, rx) = mpsc::channel();
        let mut sctx = SpoolerContext {
            bsends: senders,
            file: mf,
            uuid: params.uuid,
            sender: params.internal_tx.clone().wait(),
            rx: rx
        };
        thread::spawn(move || {
            sctx.spool();
        });
        self.senders = plains;
        self.control = Some(tx);
        Ok(true)
    }
    fn execute(&mut self, time: u64, _: ControllerParams) -> BackendResult<bool> {
        for sender in self.senders.iter_mut() {
            sender.set_start_time(time);
            sender.set_active(true);
        }
        Ok(true)
    }
    fn pause(&mut self, _: ControllerParams) {
        for sender in self.senders.iter_mut() {
            sender.set_active(false);
        }
    }
    fn reset(&mut self, _: ControllerParams) {
        for _ in self.senders.drain(..) {}
        if let Some(c) = self.control.take() {
            c.send(SpoolerMessage::Quit);
        }
    }
    fn duration(&self) -> Option<Duration> {
        if let Some(s) = self.senders.get(0) {
            if let Ok(d) = s.position().to_std() {
                return Some(d);
            }
        }
        None
    }
    fn accept_audio_message(&mut self, msg: &AudioThreadMessage, ctx: ControllerParams) -> bool {
        use self::AudioThreadMessage::*;
        match *msg {
            PlayerBufHalf(uu) | PlayerBufEmpty(uu) => {
                for sender in self.senders.iter() {
                    if sender.uuid() == uu {
                        if let Some(c) = self.control.as_mut() {
                            if let Err(e) = c.send(SpoolerMessage::Wakeup) {
                                let msg = format!("Failed to wakeup spooler thread: {:?}", e);
                                let fut = ctx.internal_tx.clone().send(
                                    ServerMessage::ActionWarning(ctx.uuid,
                                                                 msg));
                                ctx.ctx.remote.spawn(move |_| {
                                    fut.map_err(|_| ()).map(|_| ())
                                });
                            }
                        }
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }
}
