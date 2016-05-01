SQA: stuttery QLab alternative
==============================
*Stability: "We maintain a 99.9% non-feature-parity with other audio solutions! That's right, almost nothing works!*

![screenshot](http://i.imgur.com/YEF8xTl.jpg)

*SQA running under [cool-retro-term](https://github.com/Swordfish90/cool-retro-term), playing back 3 different files with ease.*

## wat

This project aims to be an audio player & cue system for live shows and staged productions,
à la Figure53's [QLab](http://figure53.com/qlab/).

Please note that, despite its name, this project probably won't reach the feature count, stability, and market share
of QLab - it's mainly intended to be a fun side project. I'd love it to, though!

## Cool! Does it work?

It doesn't have any QLab-like features yet (like cues), but you *can* play around with the audio engine, and there's a nice
command line interface (pictured above). The audio engine works quite well, but can and will be altered slightly to increase
playback quality and smoothness.

This uses my [rsndfile](https://github.com/eeeeeta/rsndfile) bindings to provide audio loading & data extraction.
These bindings weren't created to do much other than serve this project - but you may find them somewhat useful.

## How do I play with it?

SQA requires a **nightly** version of [Rust](https://www.rust-lang.org/) to run. See [the pertinent Rust Book](https://doc.rust-lang.org/book/nightly-rust.html) section.

Thankfully, SQA uses the awesome power of [Cargo](https://crates.io/), so compilation and usage is fairly simple. Note however
the fact that you need to have rsndfile accessible at `../rsndfile` - this is handy for my development of SQA and I can't
be arsed to find a better solution. Code:

    $ mkdir sqa-stuff
    $ git clone https://github.com/eeeeeta/sqa
    $ git clone https://github.com/eeeeeta/rsndfile
    $ cd sqa
    $ cargo build

Now, put your favourite AIFF files (or other libsndfile supported format) inside the current working directory.

    $ cargo run

If it's all worked out alright, you'll now see a lovely interface. Instructions for use:

- The yellow line of text displays the last error encountered. If you can't type, see if it has something to say.
- Use the following keybindings to insert tokens:
- l for LOAD
- a for AS
- p for POS
- s for START
- o for STOP
- c for CHAN
- v for VOL
- @ for @
- `$[text] ` to insert an identifier
- `"path" ` to insert a file path
- [-]0..9 for numbers
- The following commands are supported:
- LOAD path [AS identifier]
- VOL identifier [CHAN number] @ decibels [FADE seconds]
- POS identifier @ seconds
- START [identifier]
- STOP [identifier]

Good luck, and have fun.

## Documentation plz.

You should be able to use `rustdoc` and Cargo to generate docs:

    $ cargo doc
    $ cargo rustdoc -- --no-defaults --passes "collapse-docs" --passes "unindent-comments"

Check the `target/doc/sqa` folder for output.
