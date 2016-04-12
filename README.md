SQA: stuttery QLab alternative
==============================
*Stability: "We maintain a 99.9% non-feature-parity with other audio solutions! That's right, almost nothing works!*

## wat

This project aims to be an audio player & cue system for live shows and staged productions,
à la Figure53's [QLab](http://figure53.com/qlab/).

Please note that, despite its name, this project probably won't reach the feature count, stability, and market share
of QLab - it's mainly intended to be a fun side project. Please also note that it doesn't stutter on my machine (yet) -
I just wanted to have a title that didn't contain expletives.

## Cool! Does it work?

Uh...

All it does currently is plays back two audio streams ~~fading in over each other~~ in different channels,
with one of them pausing and jumping and doing random stuff. While it may not look like much, that's
actually a significant amount of work in multithreading and sound control. It's coming along, and there'll be a
hopefully decent CLI for it soon.

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

At this point, put your favourite music (as `test.aiff`) and favourite cats meowing sound effects (as `meows.aiff`) in the `sqa`
folder we're in (inside the `sqa-stuff` folder we created). These do both need to be AIFF files.

    $ cargo run

You should now hear your music fading in, accompanied by the wild mewling of cats. If you don't, raise an issue.

## Documentation plz.

You should be able to use `rustdoc` and Cargo to generate docs:

    $ cargo doc
    $ cargo rustdoc -- --no-defaults --passes "collapse-docs" --passes "unindent-comments"

Check the `target/doc/sqa` folder for output.
