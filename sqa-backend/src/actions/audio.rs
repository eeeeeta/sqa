//! Plays back an audio file.

use sqa_engine::{PlainSender, BufferSender};
use sqa_engine::sync::AudioThreadMessage;
use sqa_ffmpeg::{Frame, MediaFile, MediaResult};
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
    control: Option<Sender<SpoolerMessage>>,
    file: Option<MediaResult<MediaFile>>
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AudioChannel {
    pub patch: Option<Uuid>,
    pub vol: f32
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AudioParams {
    pub url: Option<String>,
    pub chans: Vec<AudioChannel>
}

impl Controller {
    pub fn new() -> Self {
        Controller {
            params: AudioParams {
                url: None,
                chans: vec![],
            },
            senders: vec![],
            control: None,
            file: None
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
    fn set_params(&mut self, mut p: AudioParams, ctx: &mut ActionContext) {
        if self.params.url != p.url {
            self.file = match p.url {
                Some(ref st) => {
                    let mf = MediaFile::new(ctx.media, &st);
                    match mf {
                        Err(e) => Some(Err(e)),
                        Ok(mf) => {
                            if p.chans.len() != mf.channels() {
                                if p.chans.len() == 0 {
                                    p.chans = vec![Default::default(); mf.channels()];
                                }
                                else if p.chans.len() < mf.channels() {
                                    let len = p.chans.len();
                                    p.chans.extend(::std::iter::repeat(Default::default())
                                                             .take(mf.channels() - len));
                                }
                            }
                            Some(Ok(mf))
                        }
                    }
                },
                None => None
            };
        }
        self.params = p;
    }
    fn verify_params(&self, ctx: &mut ActionContext) -> Vec<ParameterError> {
        let mut ret = vec![];
        if self.params.url.is_some() {
            let mf = match self.file.as_ref() {
                Some(f) => f,
                None => {
                    return vec![ParameterError {
                        name: "url".into(),
                        err: format!("Internal error: no file opened!")
                    }]
                }
            };
            match *mf {
                Err(ref e) => {
                    ret.push(ParameterError {
                        name: "url".into(),
                        err: format!("Error opening URL: {}", e)
                    })
                },
                Ok(ref mf) => {
                    if mf.channels() == 0 {
                        ret.push(ParameterError {
                            name: "url".into(),
                            err: "What sort of file has exactly ZERO channels in it?!".into()
                        });
                    }
                    if self.params.chans.len() > mf.channels() {
                        ret.push(ParameterError {
                            name: "chans".into(),
                            err: "The file has less channels than expected (FIXME: better error message here)".into()
                        });
                    }
                }
            }
            for (i, ch) in self.params.chans.iter().enumerate() {
                if let Some(ref ch) = ch.patch {
                    if ctx.mixer.obtain_channel(&ch).is_none() {
                        ret.push(ParameterError {
                            name: "chans".into(),
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
            if let Some(ch) = self.params.chans.get(i) {
                if let Some(ref ch) = ch.patch {
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
