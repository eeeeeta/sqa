//! Plays back an audio file.

use sqa_engine::{PlainSender, Sender};
use sqa_ffmpeg::MediaFile;
use super::{ParameterError, ActionFuture, LoadFuture, ActionController};
use state::Context;
use futures::future;
use futures::Future;
use std::error::Error;
use std::any::Any;
use futures::sync::{mpsc, oneshot};
use std::thread;
use std::panic;
pub struct Controller {
    params: AudioParams,
    senders: Vec<PlainSender>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AudioParams {
    url: Option<String>
}

impl Controller {
    fn load(file: MediaFile) -> Result<Vec<PlainSender>, Box<Error + Send>> {
        unimplemented!()
    }
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
    fn verify_params(&self, ctx: &mut Context) -> Vec<ParameterError> {
        let mut ret = vec![];
        if let Some(ref st) = self.params.url {
            let mf = MediaFile::new(&mut ctx.media, &st);
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
    fn load(&mut self, ctx: &mut Context) -> Result<Option<LoadFuture>, Box<Error>> {
        let (tx, rx) = oneshot::channel();
        let url = self.params.url.clone().unwrap();
        let mf = MediaFile::new(&mut ctx.media, &url)?;
        thread::spawn(move || {
            tx.complete(panic::catch_unwind(|| {
                Controller::load(mf)
            }));
        });
        let fut = rx.map_err(|c| {
            panic!("wat")
        }).and_then(|res| {
            match res {
                Ok(x) => match x {
                    Ok(y) => Ok(Box::new(y) as _),
                    Err(e) => Err(e)
                },
                Err(e) => Err(Box::new(::std::io::Error::new(::std::io::ErrorKind::Other, "failure")) as _)
            }
        });
        Ok(Some(Box::new(fut)))
    }
    fn loaded(&mut self, ctx: &mut Context, a: Box<Any>) -> Result<(), Box<Error>> {
        unimplemented!()
    }
    fn execute(&mut self, time: u64, ctx: &mut Context) -> ActionFuture {
        unimplemented!()
    }
}
